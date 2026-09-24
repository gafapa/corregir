-- Esquema inicial (Hito 2). Ver docs/ARQUITECTURA.md para el diseño completo
-- del pipeline y docs/DPIA-EIPD.md para las razones de cada medida de
-- minimización (p. ej. por qué los documentos originales no viven aquí).

CREATE TABLE IF NOT EXISTS rubricas (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    titulo TEXT NOT NULL,
    asignatura TEXT NOT NULL,
    curso TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    contenido_json TEXT NOT NULL,
    creado_en TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS criterios_rubrica (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    rubrica_id INTEGER NOT NULL REFERENCES rubricas(id) ON DELETE CASCADE,
    codigo TEXT NOT NULL,
    descripcion TEXT NOT NULL,
    puntuacion_max REAL NOT NULL,
    orden INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS enunciados (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    rubrica_id INTEGER NOT NULL REFERENCES rubricas(id) ON DELETE CASCADE,
    texto TEXT NOT NULL,
    materiales_ref TEXT
);

-- Los documentos originales NUNCA se guardan aquí (ver ARQUITECTURA.md,
-- "frontera de privacidad"): viven en una carpeta temporal fuera de la BD,
-- purgada explícitamente (Hito 8).
CREATE TABLE IF NOT EXISTS entregas (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    enunciado_id INTEGER NOT NULL REFERENCES enunciados(id) ON DELETE CASCADE,
    alias TEXT UNIQUE,
    texto_ocr TEXT,
    metodo_ocr TEXT,
    texto_anonimizado TEXT,
    estado_pipeline TEXT NOT NULL DEFAULT 'importada',
    creado_en TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- La tabla más sensible: alias aleatorio (no correlativo) -> identidad real.
-- Nunca debe exportarse ni salir del dispositivo.
CREATE TABLE IF NOT EXISTS alias_alumno_map (
    alias TEXT PRIMARY KEY,
    alumno_nombre TEXT NOT NULL,
    alumno_id_clase TEXT,
    entrega_id INTEGER NOT NULL REFERENCES entregas(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS resultados (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entrega_id INTEGER NOT NULL REFERENCES entregas(id) ON DELETE CASCADE,
    criterio_id INTEGER NOT NULL REFERENCES criterios_rubrica(id),
    evidencia_ia_json TEXT,
    -- Reservada para el futuro modo "Corrección asistida por IA" (alto
    -- riesgo, fuera de alcance de la Fase A). Sin usar mientras el modo
    -- activo sea "Asistente de corrección".
    puntuacion_sugerida REAL,
    puntuacion_final REAL,
    comentario_ia TEXT,
    comentario_docente TEXT,
    confirmado_por_docente INTEGER NOT NULL DEFAULT 0,
    confirmado_en TEXT,
    UNIQUE(entrega_id, criterio_id)
);

-- Trazabilidad exigida por el Art. 12 del Reglamento de IA.
CREATE TABLE IF NOT EXISTS logs_auditoria (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entrega_id INTEGER REFERENCES entregas(id) ON DELETE SET NULL,
    evento TEXT NOT NULL,
    actor TEXT NOT NULL,
    version_modelo TEXT,
    payload_json TEXT,
    creado_en TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS configuracion (
    clave TEXT PRIMARY KEY,
    valor TEXT NOT NULL
);
