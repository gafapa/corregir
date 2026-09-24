use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NuevoCriterio {
    pub codigo: String,
    pub descripcion: String,
    pub puntuacion_max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NuevaRubrica {
    pub titulo: String,
    pub asignatura: String,
    pub curso: String,
    pub criterios: Vec<NuevoCriterio>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterioConId {
    pub id: i64,
    pub codigo: String,
    pub descripcion: String,
    pub puntuacion_max: f64,
    pub orden: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricaConCriterios {
    pub id: i64,
    pub titulo: String,
    pub asignatura: String,
    pub curso: String,
    pub version: i64,
    pub criterios: Vec<CriterioConId>,
}
