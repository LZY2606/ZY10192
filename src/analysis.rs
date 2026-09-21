use crate::geometry::{
    transform_point, Convention, PointInput, PointObservation, CONSTANTS_VERSION, EQUATION_VERSION,
    LAMBDA_235, LAMBDA_238, U238_U235,
};
use crate::intersection::IntersectionSolution;
use crate::regression::{fit_york, RegressionResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub common_lead_default: String,
    #[serde(default)]
    pub native_convention: Option<Convention>,
    pub points: Vec<PointInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointDiagnostic {
    pub id: String,
    pub name: String,
    pub included: bool,
    pub used_in_regression: bool,
    pub source_convention: Convention,
    pub mean: [f64; 2],
    pub cov: crate::geometry::Cov2,
    pub sx: f64,
    pub sy: f64,
    pub rho: f64,
    pub valid: bool,
    pub positive_definite: bool,
    pub errors: Vec<String>,
    pub ellipse: Option<crate::geometry::Ellipse>,
    pub lead_source: String,
    pub lead_note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regression_point: Option<crate::regression::RegressionPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRequest {
    pub dataset_id: String,
    pub source_convention: Convention,
    pub target_convention: Convention,
    pub included_point_ids: Vec<String>,
    #[serde(default = "default_scatter")]
    pub scatter_model: String,
}

fn default_scatter() -> String {
    "analytical_or_mswd".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResponse {
    pub constants: ConstantsFingerprint,
    pub request: AnalysisRequest,
    pub points: Vec<PointDiagnostic>,
    pub regression: Option<RegressionResult>,
    pub intersections: Option<IntersectionSolution>,
    pub warning: Option<String>,
    pub run_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstantsFingerprint {
    pub constants_version: String,
    pub equation_version: String,
    pub lambda_238_per_year: f64,
    pub lambda_235_per_year: f64,
    pub u238_u235: f64,
}

pub fn constants_fingerprint() -> ConstantsFingerprint {
    ConstantsFingerprint {
        constants_version: CONSTANTS_VERSION.to_string(),
        equation_version: EQUATION_VERSION.to_string(),
        lambda_238_per_year: LAMBDA_238,
        lambda_235_per_year: LAMBDA_235,
        u238_u235: U238_U235,
    }
}

pub fn analyze(request: AnalysisRequest, dataset: &Dataset) -> anyhow::Result<AnalysisResponse> {
    if dataset.id != request.dataset_id {
        anyhow::bail!("数据集 ID 不匹配");
    }
    let mut diagnostics = Vec::new();
    let mut fitting_points = Vec::new();
    for input in &dataset.points {
        let included = request.included_point_ids.iter().any(|id| id == &input.id);
        let source = PointObservation::from_input(input.clone(), request.source_convention);
        let target = transform_point(
            &source,
            request.source_convention,
            request.target_convention,
        )?;
        let used = included && target.valid && target.positive_definite;
        let regression_point = None;
        if used {
            fitting_points.push(target.clone());
        }
        diagnostics.push(PointDiagnostic {
            id: input.id.clone(),
            name: input.name.clone(),
            included,
            used_in_regression: used,
            source_convention: request.source_convention,
            mean: target.mean,
            cov: target.cov,
            sx: target.input.sx,
            sy: target.input.sy,
            rho: target.input.rho,
            valid: target.valid,
            positive_definite: target.positive_definite,
            errors: target.errors.clone(),
            ellipse: crate::geometry::ellipse(&target),
            lead_source: input.lead_source.clone(),
            lead_note: input.lead_note.clone(),
            regression_point,
        });
    }

    let mut warning = None;
    let regression = match fit_york(&fitting_points) {
        Ok(value) => Some(value),
        Err(error) => {
            warning = Some(error.to_string());
            None
        }
    };
    let intersections = regression
        .as_ref()
        .map(|reg| crate::intersection::solve_intersections(request.target_convention, reg));

    if let Some(reg) = regression.as_ref() {
        for diagnostic in &mut diagnostics {
            diagnostic.regression_point = reg
                .points
                .iter()
                .find(|point| point.id == diagnostic.id)
                .cloned();
        }
    }

    let fingerprint_payload = serde_json::json!({
        "constants": constants_fingerprint(),
        "request": &request,
        "dataset_id": dataset.id,
        "points": dataset.points,
    });
    let serialized = serde_json::to_string(&fingerprint_payload)?;
    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    let run_fingerprint = format!("{:x}", hasher.finalize());

    Ok(AnalysisResponse {
        constants: constants_fingerprint(),
        request,
        points: diagnostics,
        regression,
        intersections,
        warning,
        run_fingerprint,
    })
}
