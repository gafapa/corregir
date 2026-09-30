-- Rename the original Spanish schema in place so existing encrypted databases
-- retain rubrics, submissions, grades, and audit history.

ALTER TABLE rubricas RENAME TO rubrics;
ALTER TABLE rubrics RENAME COLUMN titulo TO title;
ALTER TABLE rubrics RENAME COLUMN asignatura TO subject;
ALTER TABLE rubrics RENAME COLUMN curso TO grade_level;
ALTER TABLE rubrics RENAME COLUMN contenido_json TO content_json;
ALTER TABLE rubrics RENAME COLUMN creado_en TO created_at;

ALTER TABLE criterios_rubrica RENAME TO criteria_rubric;
ALTER TABLE criteria_rubric RENAME COLUMN rubrica_id TO rubric_id;
ALTER TABLE criteria_rubric RENAME COLUMN codigo TO code;
ALTER TABLE criteria_rubric RENAME COLUMN descripcion TO description;
ALTER TABLE criteria_rubric RENAME COLUMN puntuacion_max TO score_max;
ALTER TABLE criteria_rubric RENAME COLUMN orden TO "sort_order";

ALTER TABLE enunciados RENAME TO assignments;
ALTER TABLE assignments RENAME COLUMN rubrica_id TO rubric_id;
ALTER TABLE assignments RENAME COLUMN texto TO text;
ALTER TABLE assignments RENAME COLUMN materiales_ref TO materials_ref;

ALTER TABLE entregas RENAME TO submissions;
ALTER TABLE submissions RENAME COLUMN enunciado_id TO assignment_id;
ALTER TABLE submissions RENAME COLUMN texto_ocr TO text_ocr;
ALTER TABLE submissions RENAME COLUMN metodo_ocr TO method_ocr;
ALTER TABLE submissions RENAME COLUMN texto_anonimizado TO text_redacted;
ALTER TABLE submissions RENAME COLUMN estado_pipeline TO status_pipeline;
ALTER TABLE submissions RENAME COLUMN creado_en TO created_at;

ALTER TABLE alias_alumno_map RENAME TO alias_student_map;
ALTER TABLE alias_student_map RENAME COLUMN alumno_nombre TO student_name;
ALTER TABLE alias_student_map RENAME COLUMN alumno_id_clase TO student_class_id;
ALTER TABLE alias_student_map RENAME COLUMN entrega_id TO submission_id;

ALTER TABLE resultados RENAME TO results;
ALTER TABLE results RENAME COLUMN entrega_id TO submission_id;
ALTER TABLE results RENAME COLUMN criterio_id TO criterion_id;
ALTER TABLE results RENAME COLUMN evidencia_ia_json TO ai_evidence_json;
ALTER TABLE results RENAME COLUMN puntuacion_sugerida TO score_suggested;
ALTER TABLE results RENAME COLUMN puntuacion_final TO score_final;
ALTER TABLE results RENAME COLUMN comentario_ia TO ai_comment;
ALTER TABLE results RENAME COLUMN comentario_docente TO comment_teacher;
ALTER TABLE results RENAME COLUMN confirmado_por_docente TO confirmed_by_teacher;
ALTER TABLE results RENAME COLUMN confirmado_en TO confirmed_at;

ALTER TABLE logs_auditoria RENAME TO logs_audit;
ALTER TABLE logs_audit RENAME COLUMN entrega_id TO submission_id;
ALTER TABLE logs_audit RENAME COLUMN evento TO event;
ALTER TABLE logs_audit RENAME COLUMN version_modelo TO model_version;
ALTER TABLE logs_audit RENAME COLUMN creado_en TO created_at;

ALTER TABLE configuracion RENAME TO configuration;
ALTER TABLE configuration RENAME COLUMN clave TO key;
ALTER TABLE configuration RENAME COLUMN valor TO value;

UPDATE submissions SET status_pipeline = CASE status_pipeline
    WHEN 'importada' THEN 'imported'
    WHEN 'ocr_completado' THEN 'ocr_complete'
    WHEN 'anonimizada' THEN 'redacted'
    WHEN 'nota_confirmada' THEN 'grade_confirmed'
    ELSE status_pipeline END;

UPDATE logs_audit SET actor = CASE actor
    WHEN 'sistema' THEN 'system'
    WHEN 'profesor' THEN 'teacher'
    WHEN 'ia' THEN 'ai'
    ELSE actor END;
UPDATE logs_audit SET event = CASE event
    WHEN 'ocr_completado' THEN 'ocr_complete'
    WHEN 'anonimizacion_confirmada' THEN 'redaction_confirmed'
    WHEN 'nota_confirmada' THEN 'grade_confirmed'
    WHEN 'llamada_a_evidencias' THEN 'ai_evidence_request'
    WHEN 'llamada_a_feedback' THEN 'ai_feedback_request'
    WHEN 'llamada_b_feedback' THEN 'ai_feedback_request'
    ELSE event END;
