use serde::{Deserialize, Serialize};

pub const LAMBDA_238: f64 = 1.55125e-10;
pub const LAMBDA_235: f64 = 9.8485e-10;
pub const U238_U235: f64 = 137.818;
pub const CONSTANTS_VERSION: &str = "fixed-upb-constants-v1";
pub const EQUATION_VERSION: &str = "wetherill-tw-v1";
pub const MAX_AGE_YR: f64 = 4_500_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Convention {
    Wetherill,
    TeraWasserburg,
}

impl Convention {
    pub fn label(self) -> &'static str {
        match self {
            Convention::Wetherill => "Wetherill: 207Pb/235U–206Pb/238U",
            Convention::TeraWasserburg => "Tera-Wasserburg: 238U/206Pb–207Pb/206Pb",
        }
    }

    pub fn x_label(self) -> &'static str {
        match self {
            Convention::Wetherill => "²⁰⁷Pb/²³⁵U",
            Convention::TeraWasserburg => "²³⁸U/²⁰⁶Pb",
        }
    }

    pub fn y_label(self) -> &'static str {
        match self {
            Convention::Wetherill => "²⁰⁶Pb/²³⁸U",
            Convention::TeraWasserburg => "²⁰⁷Pb/²⁰⁶Pb",
        }
    }
}

pub type Cov2 = [[f64; 2]; 2];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PointInput {
    pub id: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub sx: f64,
    pub sy: f64,
    pub rho: f64,
    #[serde(default = "default_lead_source")]
    pub lead_source: String,
    #[serde(default)]
    pub lead_note: String,
}

fn default_lead_source() -> String {
    "recorded".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PointObservation {
    pub input: PointInput,
    pub mean: [f64; 2],
    pub cov: Cov2,
    pub valid: bool,
    pub positive_definite: bool,
    pub errors: Vec<String>,
}

impl PointObservation {
    pub fn from_input(point: PointInput, convention: Convention) -> Self {
        let mut errors = Vec::new();
        for (name, value) in [
            ("x", point.x),
            ("y", point.y),
            ("sx", point.sx),
            ("sy", point.sy),
            ("rho", point.rho),
        ] {
            if !value.is_finite() {
                errors.push(format!("{name} 必须是有限数"));
            }
        }
        if point.rho.is_finite() && !(-1.0..=1.0).contains(&point.rho) {
            errors.push("相关系数 rho 必须位于闭区间 [-1, 1]".to_string());
        }
        if point.sx.is_finite() && point.sx <= 0.0 {
            errors.push("sx 必须严格为正".to_string());
        }
        if point.sy.is_finite() && point.sy <= 0.0 {
            errors.push("sy 必须严格为正".to_string());
        }
        if point.x.is_finite() && point.y.is_finite() {
            match convention {
                Convention::Wetherill if point.x < 0.0 || point.y < 0.0 => {
                    errors.push("Wetherill 比值不能为负".to_string());
                }
                Convention::TeraWasserburg if point.x <= 0.0 || point.y < 0.0 => {
                    errors.push("Tera-Wasserburg 横坐标必须为正".to_string());
                }
                _ => {}
            }
        }

        let cov = [
            [point.sx * point.sx, point.rho * point.sx * point.sy],
            [point.rho * point.sx * point.sy, point.sy * point.sy],
        ];
        let determinant = cov[0][0] * cov[1][1] - cov[0][1] * cov[0][1];
        let positive_definite =
            cov[0][0] > 0.0 && cov[1][1] > 0.0 && determinant.is_finite() && determinant > 0.0;
        if !positive_definite && errors.is_empty() {
            errors.push("协方差不是正定矩阵，不能生成误差椭圆".to_string());
        }
        let valid = errors.is_empty();
        Self {
            input: point.clone(),
            mean: [point.x, point.y],
            cov,
            valid,
            positive_definite,
            errors,
        }
    }
}

pub fn mat_vec(matrix: Cov2, vector: [f64; 2]) -> [f64; 2] {
    [
        matrix[0][0] * vector[0] + matrix[0][1] * vector[1],
        matrix[1][0] * vector[0] + matrix[1][1] * vector[1],
    ]
}

pub fn transform_point(
    point: &PointObservation,
    from: Convention,
    to: Convention,
) -> anyhow::Result<PointObservation> {
    if from == to {
        return Ok(point.clone());
    }
    let [x, y] = point.mean;
    if !x.is_finite() || !y.is_finite() {
        anyhow::bail!("非有限中心点不能变换");
    }
    let (mean, jacobian) = match from {
        Convention::Wetherill => {
            if y == 0.0 {
                anyhow::bail!("Wetherill 纵坐标为零时 Tera-Wasserburg 横坐标发散");
            }
            let mean = [1.0 / y, x / (U238_U235 * y)];
            let jacobian = [
                [0.0, -1.0 / (y * y)],
                [1.0 / (U238_U235 * y), -x / (U238_U235 * y * y)],
            ];
            (mean, jacobian)
        }
        Convention::TeraWasserburg => {
            if x == 0.0 {
                anyhow::bail!("Tera-Wasserburg 横坐标为零时 Wetherill 比值发散");
            }
            let mean = [U238_U235 * y / x, 1.0 / x];
            let jacobian = [
                [-U238_U235 * y / (x * x), U238_U235 / x],
                [-1.0 / (x * x), 0.0],
            ];
            (mean, jacobian)
        }
    };

    let mut transformed = Cov2::default();
    for i in 0..2 {
        for j in 0..2 {
            transformed[i][j] = (0..2)
                .flat_map(|k| {
                    (0..2).map(move |l| jacobian[i][k] * point.cov[k][l] * jacobian[j][l])
                })
                .sum();
        }
    }
    let determinant = transformed[0][0] * transformed[1][1] - transformed[0][1] * transformed[0][1];
    let positive_definite = transformed[0][0] > 0.0
        && transformed[1][1] > 0.0
        && determinant.is_finite()
        && determinant > 0.0;

    let sx = transformed[0][0].sqrt();
    let sy = transformed[1][1].sqrt();
    let rho = if sx > 0.0 && sy > 0.0 {
        (transformed[0][1] / (sx * sy)).clamp(-1.0, 1.0)
    } else {
        f64::NAN
    };
    let mut input = point.input.clone();
    input.x = mean[0];
    input.y = mean[1];
    input.sx = sx;
    input.sy = sy;
    input.rho = rho;
    let mut errors = point.errors.clone();
    if !point.positive_definite {
        errors.push("输入协方差非正定；仅显示变换诊断，不生成椭圆".to_string());
    }
    if !positive_definite && !errors.iter().any(|item| item.contains("正定")) {
        errors.push("变换后协方差非正定，已停止生成椭圆".to_string());
    }
    Ok(PointObservation {
        input,
        mean,
        cov: transformed,
        valid: point.valid && positive_definite,
        positive_definite,
        errors,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ellipse {
    pub center: [f64; 2],
    pub semi_major_sigma: f64,
    pub semi_minor_sigma: f64,
    pub angle_rad: f64,
    pub eigenvalue_major: f64,
    pub eigenvalue_minor: f64,
    pub chi_square_95_scale: f64,
}

pub fn ellipse(point: &PointObservation) -> Option<Ellipse> {
    if !point.positive_definite {
        return None;
    }
    let a = point.cov[0][0];
    let b = point.cov[0][1];
    let d = point.cov[1][1];
    let trace = a + d;
    let discriminant = ((a - d) * 0.5).hypot(b);
    let major = trace * 0.5 + discriminant;
    let minor = trace * 0.5 - discriminant;
    if !(major > 0.0 && minor > 0.0) {
        return None;
    }
    Some(Ellipse {
        center: point.mean,
        semi_major_sigma: major.sqrt(),
        semi_minor_sigma: minor.sqrt(),
        angle_rad: 0.5 * (2.0 * b).atan2(a - d),
        eigenvalue_major: major,
        eigenvalue_minor: minor,
        chi_square_95_scale: 5.991_464_822_566_054_f64.sqrt(),
    })
}

pub fn concordia(age_yr: f64, convention: Convention) -> [f64; 2] {
    let e7 = (LAMBDA_235 * age_yr).exp();
    let e8 = (LAMBDA_238 * age_yr).exp();
    let x7 = e7 - 1.0;
    let x8 = e8 - 1.0;
    match convention {
        Convention::Wetherill => [x7, x8],
        Convention::TeraWasserburg => [1.0 / x8, x7 / (U238_U235 * x8)],
    }
}

pub fn concordia_derivative(age_yr: f64, convention: Convention) -> [f64; 2] {
    let e7 = (LAMBDA_235 * age_yr).exp();
    let e8 = (LAMBDA_238 * age_yr).exp();
    let x7 = e7 - 1.0;
    let x8 = e8 - 1.0;
    match convention {
        Convention::Wetherill => [LAMBDA_235 * e7, LAMBDA_238 * e8],
        Convention::TeraWasserburg => [
            -LAMBDA_238 * e8 / (x8 * x8),
            (LAMBDA_235 * e7 * x8 - x7 * LAMBDA_238 * e8) / (U238_U235 * x8 * x8),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_rho_outside_closed_interval() {
        let p = PointObservation::from_input(
            PointInput {
                id: "bad".into(),
                name: "bad".into(),
                x: 1.0,
                y: 0.1,
                sx: 0.02,
                sy: 0.003,
                rho: 1.2,
                lead_source: "recorded".into(),
                lead_note: String::new(),
            },
            Convention::Wetherill,
        );
        assert!(!p.valid);
        assert!(ellipse(&p).is_none());
    }

    #[test]
    fn transforms_means_and_covariance_and_round_trips() {
        let original = PointObservation::from_input(
            PointInput {
                id: "p".into(),
                name: "p".into(),
                x: 1.4,
                y: 0.13,
                sx: 0.03,
                sy: 0.004,
                rho: -0.35,
                lead_source: "Stacey–Kramers".into(),
                lead_note: String::new(),
            },
            Convention::Wetherill,
        );
        let tw =
            transform_point(&original, Convention::Wetherill, Convention::TeraWasserburg).unwrap();
        assert!(tw.positive_definite);
        assert!(ellipse(&tw).is_some());
        let back = transform_point(&tw, Convention::TeraWasserburg, Convention::Wetherill).unwrap();
        assert!((back.mean[0] - original.mean[0]).abs() < 1e-12);
        assert!((back.mean[1] - original.mean[1]).abs() < 1e-12);
        for i in 0..2 {
            for j in 0..2 {
                assert!((back.cov[i][j] - original.cov[i][j]).abs() < 1e-17);
            }
        }
    }

    #[test]
    fn jacobian_propagation_matches_explicit_tw_values() {
        let original = PointObservation::from_input(
            PointInput {
                id: "explicit".into(),
                name: "explicit".into(),
                x: 1.4,
                y: 0.13,
                sx: 0.03,
                sy: 0.004,
                rho: -0.35,
                lead_source: "test".into(),
                lead_note: String::new(),
            },
            Convention::Wetherill,
        );
        let sx = 0.03_f64;
        let sy = 0.004_f64;
        let rho = -0.35_f64;
        let cov = [[sx * sx, rho * sx * sy], [rho * sx * sy, sy * sy]];
        let y = 0.13_f64;
        let x = 1.4_f64;
        let jacobian = [
            [0.0, -1.0 / (y * y)],
            [1.0 / (U238_U235 * y), -x / (U238_U235 * y * y)],
        ];
        let mut expected = [[0.0; 2]; 2];
        for i in 0..2 {
            for j in 0..2 {
                expected[i][j] = (0..2)
                    .flat_map(|k| (0..2).map(move |l| jacobian[i][k] * cov[k][l] * jacobian[j][l]))
                    .sum();
            }
        }
        let tw =
            transform_point(&original, Convention::Wetherill, Convention::TeraWasserburg).unwrap();
        for (i, row) in expected.iter().enumerate() {
            for (j, value) in row.iter().enumerate() {
                assert!((tw.cov[i][j] - value).abs() < 1e-18);
            }
        }
    }

    #[test]
    fn non_positive_definite_covariance_cannot_continue_to_ellipse() {
        let invalid = PointObservation {
            input: PointInput {
                id: "degenerate".into(),
                name: "degenerate".into(),
                x: 1.0,
                y: 0.1,
                sx: 0.02,
                sy: 0.003,
                rho: 1.0,
                lead_source: "test".into(),
                lead_note: String::new(),
            },
            mean: [1.0, 0.1],
            cov: [[0.000_4, 0.000_06], [0.000_06, 0.000_000_003_6]],
            valid: false,
            positive_definite: false,
            errors: vec!["synthetic".into()],
        };
        let transformed =
            transform_point(&invalid, Convention::Wetherill, Convention::TeraWasserburg).unwrap();
        assert!(!transformed.valid);
        assert!(ellipse(&transformed).is_none());
        assert!(ellipse(&invalid).is_none());
    }
}
