//! Detección de identificadores directos (Hito 4): reglas/regex validadas
//! (DNI/NIE con dígito de control, email, teléfono, IBAN) + coincidencia
//! contra la lista de la clase que introduce el profesor.
//!
//! Nota de alcance deliberada: el plan preveía además un modelo NER local
//! (ONNX vía `ort`) para identificadores indirectos ("soy el único alumno
//! que..."). Se deja fuera de esta fase: integrar `ort` añade otra
//! dependencia de biblioteca nativa (onnxruntime) del mismo tipo que ya dio
//! fricción con SQLCipher (Hito 2), y el propio diseño del pipeline
//! (`docs/ARQUITECTURA.md`) ya trata la detección automática como
//! insuficiente por sí sola: la pantalla de revisión humana obligatoria
//! (`pipeline::anonimizacion`) es la que realmente cierra ese hueco, con o
//! sin NER. Enchufar un modelo NER más adelante no cambia esa arquitectura,
//! solo añade más candidatos a la misma pantalla de revisión.

use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CandidatoIdentificador {
    pub tipo: String,
    pub texto: String,
    pub inicio: usize,
    pub fin: usize,
}

const TABLA_LETRA_DNI: &str = "TRWAGMYFPDXBNJZSQVHLCKE";

fn letra_dni_valida(numero: u32, letra: char) -> bool {
    TABLA_LETRA_DNI
        .chars()
        .nth((numero % 23) as usize)
        .map(|l| l.eq_ignore_ascii_case(&letra))
        .unwrap_or(false)
}

static RE_DNI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{8})[-\s]?([A-Za-z])\b").unwrap());
static RE_NIE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b([XYZxyz])[-\s]?(\d{7})[-\s]?([A-Za-z])\b").unwrap());
static RE_EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap()
});
static RE_TELEFONO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[679]\d{8}\b").unwrap());
static RE_IBAN: LazyLock<Regex> = LazyLock::new(|| {
    // Comprobación estructural (ES + 22 dígitos, con separadores opcionales);
    // no se valida el dígito de control mod-97 — suficiente para señalar el
    // candidato a la revisión humana, no para certificar que es un IBAN real.
    Regex::new(r"\bES\d{2}[ -]?(?:\d{4}[ -]?){4}\d{2}\b").unwrap()
});

/// Identificadores directos detectables por regla, con validación de
/// checksum cuando aplica (DNI/NIE).
pub fn detectar_por_regla(texto: &str) -> Vec<CandidatoIdentificador> {
    let mut candidatos = Vec::new();

    for m in RE_DNI.captures_iter(texto) {
        let total = m.get(0).unwrap();
        let numero: u32 = m[1].parse().unwrap_or(0);
        let letra = m[2].chars().next().unwrap_or(' ');
        if letra_dni_valida(numero, letra) {
            candidatos.push(CandidatoIdentificador {
                tipo: "dni".into(),
                texto: total.as_str().to_string(),
                inicio: total.start(),
                fin: total.end(),
            });
        }
    }

    for m in RE_NIE.captures_iter(texto) {
        let total = m.get(0).unwrap();
        let prefijo = match m[1].to_ascii_uppercase().as_str() {
            "X" => 0,
            "Y" => 1,
            "Z" => 2,
            _ => continue,
        };
        let resto: u32 = m[2].parse().unwrap_or(0);
        let numero = prefijo * 10_000_000 + resto;
        let letra = m[3].chars().next().unwrap_or(' ');
        if letra_dni_valida(numero, letra) {
            candidatos.push(CandidatoIdentificador {
                tipo: "nie".into(),
                texto: total.as_str().to_string(),
                inicio: total.start(),
                fin: total.end(),
            });
        }
    }

    for m in RE_EMAIL.find_iter(texto) {
        candidatos.push(CandidatoIdentificador {
            tipo: "email".into(),
            texto: m.as_str().to_string(),
            inicio: m.start(),
            fin: m.end(),
        });
    }

    for m in RE_TELEFONO.find_iter(texto) {
        candidatos.push(CandidatoIdentificador {
            tipo: "telefono".into(),
            texto: m.as_str().to_string(),
            inicio: m.start(),
            fin: m.end(),
        });
    }

    for m in RE_IBAN.find_iter(texto) {
        candidatos.push(CandidatoIdentificador {
            tipo: "iban".into(),
            texto: m.as_str().to_string(),
            inicio: m.start(),
            fin: m.end(),
        });
    }

    candidatos
}

/// Busca en `texto` cada nombre completo de `roster`, y también cada uno de
/// sus componentes (nombre, apellidos) por separado, para cubrir el caso de
/// que el alumno solo se identifique por su nombre de pila. Coincidencia
/// literal insensible a mayúsculas/minúsculas — no tolera erratas de OCR;
/// ver nota de alcance al principio del módulo.
pub fn detectar_por_roster(texto: &str, roster: &[String]) -> Vec<CandidatoIdentificador> {
    let texto_min = texto.to_lowercase();
    let mut candidatos = Vec::new();
    let mut terminos: Vec<String> = Vec::new();

    for nombre_completo in roster {
        let limpio = nombre_completo.trim();
        if limpio.is_empty() {
            continue;
        }
        terminos.push(limpio.to_string());
        for parte in limpio.split_whitespace() {
            if parte.chars().count() >= 3 {
                terminos.push(parte.to_string());
            }
        }
    }

    // Términos más largos primero, para que un nombre completo se detecte
    // como un único candidato en vez de fragmentarse en sus partes.
    terminos.sort_by_key(|t| std::cmp::Reverse(t.len()));

    for termino in &terminos {
        let termino_min = termino.to_lowercase();
        let mut desde = 0;
        while let Some(pos_relativa) = texto_min[desde..].find(&termino_min) {
            let inicio = desde + pos_relativa;
            let fin = inicio + termino_min.len();
            let solapa = candidatos.iter().any(|c: &CandidatoIdentificador| {
                inicio < c.fin && fin > c.inicio
            });
            if !solapa {
                candidatos.push(CandidatoIdentificador {
                    tipo: "nombre_roster".into(),
                    texto: texto[inicio..fin].to_string(),
                    inicio,
                    fin,
                });
            }
            desde = fin;
        }
    }

    candidatos
}

pub fn detectar_todos(texto: &str, roster: &[String]) -> Vec<CandidatoIdentificador> {
    let mut candidatos = detectar_por_regla(texto);
    candidatos.extend(detectar_por_roster(texto, roster));
    candidatos.sort_by_key(|c| c.inicio);
    candidatos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecta_dni_valido_y_rechaza_invalido() {
        // 12345678 % 23 = 14 -> tabla[14] = 'Z'
        let candidatos = detectar_por_regla("Mi DNI es 12345678Z, gracias.");
        assert_eq!(candidatos.len(), 1);
        assert_eq!(candidatos[0].tipo, "dni");

        let candidatos_invalidos = detectar_por_regla("Mi DNI es 12345678A, gracias.");
        assert!(candidatos_invalidos.is_empty());
    }

    #[test]
    fn detecta_email_y_telefono() {
        let candidatos =
            detectar_por_regla("Contacto: maria.ejemplo@correo-falso.test o al 611223344.");
        let tipos: Vec<_> = candidatos.iter().map(|c| c.tipo.as_str()).collect();
        assert!(tipos.contains(&"email"));
        assert!(tipos.contains(&"telefono"));
    }

    #[test]
    fn detecta_nombre_completo_del_roster_sin_fragmentar() {
        let roster = vec!["Maria Sintetica Lopez Ejemplo".to_string()];
        let texto = "Nombre: Maria Sintetica Lopez Ejemplo\nRespuesta: ...";
        let candidatos = detectar_por_roster(texto, &roster);
        assert_eq!(candidatos.len(), 1);
        assert_eq!(candidatos[0].texto, "Maria Sintetica Lopez Ejemplo");
    }

    #[test]
    fn detecta_solo_nombre_de_pila_si_es_lo_unico_presente() {
        let roster = vec!["Juan Perez Garcia".to_string()];
        let texto = "Hola, soy Juan y esta es mi respuesta.";
        let candidatos = detectar_por_roster(texto, &roster);
        assert_eq!(candidatos.len(), 1);
        assert_eq!(candidatos[0].texto, "Juan");
    }

    #[test]
    fn detecta_todo_junto_sobre_el_pdf_sintetico_de_alumno2() {
        let texto = "Nombre: Maria Sintetica Lopez Ejemplo\nDNI: 12345678Z (ficticio)\n\
                     Email: maria.sintetica.ejemplo@correo-falso.test\n\nRespuesta...";
        let roster = vec!["Maria Sintetica Lopez Ejemplo".to_string()];
        let candidatos = detectar_todos(texto, &roster);
        let tipos: Vec<_> = candidatos.iter().map(|c| c.tipo.as_str()).collect();
        assert!(tipos.contains(&"nombre_roster"));
        assert!(tipos.contains(&"dni"));
        assert!(tipos.contains(&"email"));
    }
}
