export interface NuevoCriterio {
  codigo: string;
  descripcion: string;
  puntuacion_max: number;
}

export interface NuevaRubrica {
  titulo: string;
  asignatura: string;
  curso: string;
  criterios: NuevoCriterio[];
}

export interface CriterioConId extends NuevoCriterio {
  id: number;
  orden: number;
}

export interface RubricaConCriterios {
  id: number;
  titulo: string;
  asignatura: string;
  curso: string;
  version: number;
  criterios: CriterioConId[];
}

export interface EntregaResumen {
  id: number;
  enunciado_id: number;
  texto_ocr: string | null;
  metodo_ocr: string | null;
  estado_pipeline: string;
}

export interface CandidatoIdentificador {
  tipo: string;
  texto: string;
  inicio: number;
  fin: number;
}

export interface EvidenciaCriterio {
  criterio_id: string;
  evidencia_textual: string[];
}

export interface Inconsistencia {
  criterio_id: string;
  observacion: string;
}

export interface FeedbackYConsistencia {
  comentario_feedback: string;
  inconsistencias: Inconsistencia[];
}

export interface LogAuditoria {
  id: number;
  entrega_id: number | null;
  evento: string;
  actor: string;
  version_modelo: string | null;
  payload_json: string | null;
  creado_en: string;
}

/** Casos de prueba sintéticos definidos en el plan (docs, Hito de pruebas). */
export const RUBRICAS_SINTETICAS: NuevaRubrica[] = [
  {
    titulo: "Análisis de poema",
    asignatura: "Lingua/Lengua",
    curso: "2º ESO",
    criterios: [
      { codigo: "C1", descripcion: "Identifica correctamente el tema principal.", puntuacion_max: 4 },
      {
        codigo: "C2",
        descripcion: "Identifica y explica al menos 2 recursos literarios con ejemplo textual.",
        puntuacion_max: 3,
      },
      { codigo: "C3", descripcion: "Coherencia y corrección expresiva.", puntuacion_max: 3 },
    ],
  },
  {
    titulo: "MRUA: velocidad y distancia",
    asignatura: "Física y Química",
    curso: "4º ESO",
    criterios: [
      { codigo: "C1", descripcion: "Selecciona correctamente las fórmulas de MRUA.", puntuacion_max: 3 },
      { codigo: "C2", descripcion: "Sustituye valores y calcula sin errores aritméticos.", puntuacion_max: 4 },
      { codigo: "C3", descripcion: "Unidades correctas e interpretación del resultado.", puntuacion_max: 3 },
    ],
  },
];
