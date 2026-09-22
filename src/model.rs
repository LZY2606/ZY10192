use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Convention {
    Wetherill,
    TeraWasserburg,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeadSource {
    None,
    Measured204,
    StaceyKramers1975,
    ProceduralBlank,
    ModelCommonLead,
    UncorrectedCommonLead,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonLead {
    pub source: LeadSource,
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pb207_pb206: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputPoint {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    pub convention: Convention,
    pub x: f64,
    pub y: f64,
    pub sigma_x: f64,
    pub sigma_y: f64,
    pub correlation: f64,
    #[serde(default)]
    pub common_lead: Option<CommonLead>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub points: Vec<InputPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportFile {
    pub schema_version: String,
    pub equation_version: String,
    pub runs: Vec<RunInput>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Covariance2 {
    pub xx: f64,
    pub xy: f64,
    pub yy: f64,
}

impl Covariance2 {
    pub fn new(xx: f64, xy: f64, yy: f64) -> Self {
        Self { xx, xy, yy }
    }

    pub fn determinant(&self) -> f64 {
        self.xx * self.yy - self.xy * self.xy
    }

    pub fn is_positive_definite(&self) -> bool {
        let scale = self.xx.abs().max(self.yy.abs()).max(1.0);
        self.xx > 1.0e-14 * scale
            && self.determinant() > 1.0e-14 * scale * scale
            && self.yy.is_finite()
            && self.xy.is_finite()
    }

    pub fn correlation(&self) -> f64 {
        self.xy / (self.xx.sqrt() * self.yy.sqrt())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CoordinatePoint {
    pub x: f64,
    pub y: f64,
    pub covariance: Covariance2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointDiagnostic {
    pub id: String,
    pub label: String,
    pub included: bool,
    pub valid: bool,
    pub errors: Vec<String>,
    pub wetherill: CoordinatePoint,
    pub tera_wasserburg: CoordinatePoint,
    pub common_lead: CommonLead,
    pub discordance_percent: Option<f64>,
    pub ellipse_available: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TraceStep {
    pub iteration: usize,
    pub theta_rad: f64,
    pub intercept_normal: f64,
    pub objective: f64,
    pub update: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionTrace {
    pub method: String,
    pub initial_theta_rad: f64,
    pub initial_intercept_normal: f64,
    pub converged: bool,
    pub iterations: usize,
    pub path: Vec<TraceStep>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EigenMode {
    pub eigenvalue: f64,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RegressionFit {
    pub slope: f64,
    pub intercept: f64,
    pub theta_rad: f64,
    pub normal_intercept: f64,
    pub covariance: Covariance2,
    pub mswd: f64,
    pub chi_square: f64,
    pub degrees_of_freedom: usize,
    pub condition_number: f64,
    pub major_mode: EigenMode,
    pub minor_mode: EigenMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResidualRow {
    pub point_id: String,
    pub label: String,
    pub normalized_residual: f64,
    pub signed_distance: f64,
    pub residual_variance: f64,
    pub leverage: f64,
    pub cooks_distance: f64,
    pub delta_lower_age_years: Option<f64>,
    pub delta_upper_age_years: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RootTraceStep {
    pub iteration: usize,
    pub lower_age_years: f64,
    pub upper_age_years: f64,
    pub f_lower: f64,
    pub f_upper: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootTrace {
    pub initial_lower_age_years: f64,
    pub initial_upper_age_years: f64,
    pub converged: bool,
    pub iterations: usize,
    pub path: Vec<RootTraceStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntersectionAge {
    pub label: String,
    pub age_years: f64,
    pub one_sigma_years: Option<f64>,
    pub stable: bool,
    pub multiplicity: usize,
    pub kind: String,
    pub error_amplification: Option<f64>,
    pub trace: RootTrace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchResult {
    pub name: String,
    pub excluded_point_ids: Vec<String>,
    pub included_count: usize,
    pub fit: Option<RegressionFit>,
    pub trace: Option<RegressionTrace>,
    pub intersections: Vec<IntersectionAge>,
    pub residual_rows: Vec<ResidualRow>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    pub equation_version: String,
    pub constants: EquationConstants,
    pub points: Vec<PointDiagnostic>,
    pub baseline: BranchResult,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EquationConstants {
    pub lambda_235_per_year: f64,
    pub lambda_238_per_year: f64,
    pub u238_over_u235: f64,
}

pub fn equation_constants() -> EquationConstants {
    EquationConstants {
        lambda_235_per_year: crate::constants::LAMBDA_235_PER_YEAR,
        lambda_238_per_year: crate::constants::LAMBDA_238_PER_YEAR,
        u238_over_u235: crate::constants::U238_OVER_U235,
    }
}
