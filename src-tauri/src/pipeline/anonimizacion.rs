//! Checkpoint de revisión humana obligatoria (Hito 4). Ningún texto puede
//! llegar al paso de IA (Hito 5) sin pasar por `confirmar` (revisión nueva)
//! o `cargar_confirmado` (recuperar una ya confirmada): son las dos únicas
//! funciones de todo el código capaces de construir un
//! `TextoAnonimizadoConfirmado`, porque su campo es privado a este módulo.
//! Ver docs/ARQUITECTURA.md, "frontera de privacidad".

use rand::RngCore;
use rusqlite::Connection;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnonimizacionError {
    #[error("la entrega {0} no existe")]
    EntregaNoExiste(i64),
    #[error("la entrega {0} no tiene texto OCR todavía")]
    SinTextoOcr(i64),
    #[error("la entrega {0} no está anonimizada (falta el checkpoint de revisión humana)")]
    NoAnonimizada(i64),
    #[error("error de base de datos: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Texto que ya ha pasado por la revisión humana de anonimización. El campo
/// es privado: solo el código de este módulo puede construir una instancia.
pub struct TextoAnonimizadoConfirmado {
    texto: String,
    alias: String,
}

impl TextoAnonimizadoConfirmado {
    pub fn texto(&self) -> &str {
        &self.texto
    }

    pub fn alias(&self) -> &str {
        &self.alias
    }
}

fn generar_alias() -> String {
    let mut bytes = [0u8; 4];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn generar_alias_unico(conn: &Connection) -> Result<String, AnonimizacionError> {
    loop {
        let candidato = generar_alias();
        let existe: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM alias_alumno_map WHERE alias = ?1)",
            [&candidato],
            |row| row.get(0),
        )?;
        if !existe {
            return Ok(candidato);
        }
    }
}

/// Fusiona rangos solapados o adyacentes en rangos disjuntos que cubren la
/// misma unión de posiciones. Imprescindible antes de redactar: los
/// candidatos de `identificadores::detectar_todos` pueden solaparse de
/// verdad (p. ej. un email que contiene un nombre del roster: "email"
/// [71..112] y el fragmento de roster "sintetica" [77..86] están dentro de
/// él), y aplicar reemplazos por posición sobre rangos solapados corrompe
/// el resultado en silencio en vez de fallar — se detectó revisando la app
/// con los propios datos sintéticos de prueba.
fn fusionar_spans(spans: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut ordenados: Vec<(usize, usize)> = spans.to_vec();
    ordenados.sort_by_key(|s| s.0);
    let mut fusionados: Vec<(usize, usize)> = Vec::new();
    for (inicio, fin) in ordenados {
        match fusionados.last_mut() {
            Some(ultimo) if inicio <= ultimo.1 => {
                ultimo.1 = ultimo.1.max(fin);
            }
            _ => fusionados.push((inicio, fin)),
        }
    }
    fusionados
}

/// Sustituye cada rango `(inicio, fin)` (offsets de byte, extremos válidos de
/// carácter UTF-8) por `alias`. Los rangos se fusionan primero (ver
/// `fusionar_spans`) para que sean disjuntos, y luego se procesan de atrás
/// hacia adelante para que un reemplazo no invalide los offsets de los
/// siguientes.
fn redactar(texto: &str, spans: &[(usize, usize)], alias: &str) -> String {
    let mut fusionados = fusionar_spans(spans);
    fusionados.sort_by_key(|s| std::cmp::Reverse(s.0));
    let mut resultado = texto.to_string();
    for (inicio, fin) in fusionados {
        resultado.replace_range(inicio..fin, alias);
    }
    resultado
}

/// El checkpoint en sí: el profesor ha revisado y confirmado qué rangos del
/// texto se redactan. Persiste el resultado y el mapa alias↔alumno, y
/// devuelve el texto ya listo para el siguiente paso del pipeline.
pub fn confirmar(
    conn: &Connection,
    entrega_id: i64,
    spans_a_redactar: &[(usize, usize)],
    alumno_nombre: &str,
    alumno_id_clase: Option<&str>,
) -> Result<TextoAnonimizadoConfirmado, AnonimizacionError> {
    let texto_original: Option<String> = conn
        .query_row(
            "SELECT texto_ocr FROM entregas WHERE id = ?1",
            [entrega_id],
            |row| row.get(0),
        )
        .map_err(|_| AnonimizacionError::EntregaNoExiste(entrega_id))?;
    let texto_original = texto_original.ok_or(AnonimizacionError::SinTextoOcr(entrega_id))?;

    let alias = generar_alias_unico(conn)?;
    let texto_anonimizado = redactar(&texto_original, spans_a_redactar, &alias);

    conn.execute(
        "UPDATE entregas SET texto_anonimizado = ?1, alias = ?2, estado_pipeline = 'anonimizada' WHERE id = ?3",
        rusqlite::params![texto_anonimizado, alias, entrega_id],
    )?;
    conn.execute(
        "INSERT INTO alias_alumno_map (alias, alumno_nombre, alumno_id_clase, entrega_id) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![alias, alumno_nombre, alumno_id_clase, entrega_id],
    )?;
    conn.execute(
        "INSERT INTO logs_auditoria (entrega_id, evento, actor, payload_json) VALUES (?1, 'anonimizacion_confirmada', 'profesor', ?2)",
        rusqlite::params![
            entrega_id,
            serde_json::json!({ "num_spans_redactados": spans_a_redactar.len() }).to_string()
        ],
    )?;

    Ok(TextoAnonimizadoConfirmado {
        texto: texto_anonimizado,
        alias,
    })
}

/// Recupera una confirmación ya persistida. Falla si la entrega no ha
/// pasado por `confirmar` — es la comprobación que impide que el Hito 5 use
/// un texto no revisado.
pub fn cargar_confirmado(
    conn: &Connection,
    entrega_id: i64,
) -> Result<TextoAnonimizadoConfirmado, AnonimizacionError> {
    let fila = conn.query_row(
        "SELECT texto_anonimizado, alias, estado_pipeline FROM entregas WHERE id = ?1",
        [entrega_id],
        |row| {
            let texto: Option<String> = row.get(0)?;
            let alias: Option<String> = row.get(1)?;
            let estado: String = row.get(2)?;
            Ok((texto, alias, estado))
        },
    );

    let (texto, alias, estado) = match fila {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(AnonimizacionError::EntregaNoExiste(entrega_id))
        }
        Err(e) => return Err(e.into()),
    };

    match (estado.as_str(), texto, alias) {
        ("anonimizada", Some(texto), Some(alias)) => {
            Ok(TextoAnonimizadoConfirmado { texto, alias })
        }
        _ => Err(AnonimizacionError::NoAnonimizada(entrega_id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;
    use tempfile::tempdir;

    fn preparar_entrega_de_prueba(conn: &Connection, texto_ocr: &str) -> i64 {
        conn.execute(
            "INSERT INTO rubricas (titulo, asignatura, curso, contenido_json) VALUES ('t','a','c','{}')",
            [],
        )
        .unwrap();
        let rubrica_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO enunciados (rubrica_id, texto) VALUES (?1, 'enunciado de prueba')",
            [rubrica_id],
        )
        .unwrap();
        let enunciado_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO entregas (enunciado_id, texto_ocr, estado_pipeline) VALUES (?1, ?2, 'ocr_completado')",
            rusqlite::params![enunciado_id, texto_ocr],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn confirmar_redacta_y_persiste_el_alias() {
        let dir = tempdir().unwrap();
        let estado = schema::abrir_con_clave(dir.path(), [9u8; 32]).unwrap();
        let guard = estado.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        let texto = "Nombre: Maria Lopez. Respuesta: el poema trata de...";
        let entrega_id = preparar_entrega_de_prueba(conn, texto);

        // "Maria Lopez" está en offsets 8..19 (comprobado por construcción del texto).
        let inicio = texto.find("Maria Lopez").unwrap();
        let fin = inicio + "Maria Lopez".len();

        let confirmado = confirmar(conn, entrega_id, &[(inicio, fin)], "Maria Lopez", None).unwrap();

        assert!(!confirmado.texto().contains("Maria Lopez"));
        assert!(confirmado.texto().contains(confirmado.alias()));
        assert!(confirmado.texto().contains("el poema trata de"));

        // El mapa alias->alumno debe existir y apuntar al nombre real.
        let nombre_real: String = conn
            .query_row(
                "SELECT alumno_nombre FROM alias_alumno_map WHERE alias = ?1",
                [confirmado.alias()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(nombre_real, "Maria Lopez");
    }

    #[test]
    fn cargar_confirmado_falla_si_no_se_ha_confirmado_antes() {
        let dir = tempdir().unwrap();
        let estado = schema::abrir_con_clave(dir.path(), [9u8; 32]).unwrap();
        let guard = estado.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        let entrega_id = preparar_entrega_de_prueba(conn, "texto sin anonimizar todavia");

        let resultado = cargar_confirmado(conn, entrega_id);
        assert!(matches!(resultado, Err(AnonimizacionError::NoAnonimizada(_))));
    }

    #[test]
    fn cargar_confirmado_funciona_tras_confirmar() {
        let dir = tempdir().unwrap();
        let estado = schema::abrir_con_clave(dir.path(), [9u8; 32]).unwrap();
        let guard = estado.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        let entrega_id = preparar_entrega_de_prueba(conn, "sin identificadores en este texto");
        confirmar(conn, entrega_id, &[], "Alumno Anonimo", None).unwrap();

        let recuperado = cargar_confirmado(conn, entrega_id).unwrap();
        assert_eq!(recuperado.texto(), "sin identificadores en este texto");
    }

    /// Regresión: `identificadores::detectar_todos` puede devolver candidatos
    /// solapados de verdad (un email que contiene fragmentos de un nombre del
    /// roster — ver pipeline::identificadores::tests::
    /// diagnostico_solapamiento_email_vs_roster para los spans exactos).
    /// Antes de fusionar_spans, esto corrompía el texto anonimizado en
    /// silencio (dejaba fragmentos de alias mezclados, p. ej.
    /// "maria.sintetica.ejemplo@..." quedaba como "d1f79bc5bc5" en vez de
    /// redactarse limpiamente).
    #[test]
    fn confirmar_con_spans_solapados_no_corrompe_el_texto() {
        let dir = tempdir().unwrap();
        let estado = schema::abrir_con_clave(dir.path(), [13u8; 32]).unwrap();
        let guard = estado.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        let texto = "Nombre: Maria Sintetica Lopez Ejemplo\nDNI: 12345678Z (ficticio)\n\
                     Email: maria.sintetica.ejemplo@correo-falso.test\n\nRespuesta...";
        let entrega_id = preparar_entrega_de_prueba(conn, texto);

        // Spans reales detectados por identificadores::detectar_todos para este
        // mismo texto: el email [71..112] solapa con los fragmentos de roster
        // "sintetica" [77..86] y "ejemplo" [87..94], y comparte inicio con
        // "maria" [71..76].
        let spans = vec![(8, 37), (43, 52), (71, 112), (71, 76), (77, 86), (87, 94)];

        let confirmado =
            confirmar(conn, entrega_id, &spans, "Maria Sintetica Lopez Ejemplo", None).unwrap();
        let texto_final = confirmado.texto();

        assert!(texto_final.contains("Respuesta..."), "no debe tocar texto fuera de los spans");
        assert!(
            !texto_final.contains("maria.sintetica.ejemplo"),
            "el email no debe sobrevivir de ninguna forma: {texto_final}"
        );
        assert!(
            !texto_final.contains("correo-falso.test"),
            "el dominio del email no debe sobrevivir: {texto_final}"
        );
        // Cada aparición del alias debe tener exactamente 8 caracteres hex,
        // nunca fragmentos pegados de un reemplazo solapado mal resuelto.
        for aparicion in texto_final.match_indices(confirmado.alias()) {
            let siguiente = texto_final.as_bytes().get(aparicion.0 + confirmado.alias().len());
            let es_hex_extra = siguiente.is_some_and(|b| b.is_ascii_hexdigit());
            assert!(!es_hex_extra, "alias con cola pegada (solape mal resuelto): {texto_final}");
        }
    }
}
