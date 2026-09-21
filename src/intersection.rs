use crate::geometry::{concordia, concordia_derivative, Convention, Cov2, MAX_AGE_YR};
use crate::regression::RegressionResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntersectionKind {
    Stable,
    TangentUnstable,
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootTrace {
    pub iteration: usize,
    pub low_age_yr: f64,
    pub high_age_yr: f64,
    pub f_low: f64,
    pub f_high: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intersection {
    pub age_yr: f64,
    pub age_ma: f64,
    pub concordia_point: [f64; 2],
    pub kind: IntersectionKind,
    pub stable: bool,
    pub sigma_age_yr: Option<f64>,
    pub sigma_age_ma: Option<f64>,
    pub age_intercept_derivative: f64,
    pub age_slope_derivative: f64,
    pub line_one_sigma: f64,
    pub time_derivative: f64,
    pub error_amplification: Option<f64>,
    pub residual_function: f64,
    pub root_trace: Vec<RootTrace>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntersectionSolution {
    pub convention: Convention,
    pub intercept: f64,
    pub slope: f64,
    pub intersections: Vec<Intersection>,
    pub diagnostic: String,
}

fn residual(age_yr: f64, convention: Convention, intercept: f64, slope: f64) -> f64 {
    let [x, y] = concordia(age_yr, convention);
    y - (intercept + slope * x)
}

fn residual_derivative(age_yr: f64, convention: Convention, _intercept: f64, slope: f64) -> f64 {
    let [dx, dy] = concordia_derivative(age_yr, convention);
    dy - slope * dx
}

fn bisect(
    mut low: f64,
    mut high: f64,
    intercept: f64,
    slope: f64,
    convention: Convention,
    derivative: bool,
) -> (f64, Vec<RootTrace>) {
    let f = |age| {
        if derivative {
            residual_derivative(age, convention, intercept, slope)
        } else {
            residual(age, convention, intercept, slope)
        }
    };
    let mut trace = Vec::new();
    let mut f_low = f(low);
    let mut f_high = f(high);
    for iteration in 0..200 {
        let mid = (low + high) * 0.5;
        let f_mid = f(mid);
        trace.push(RootTrace {
            iteration,
            low_age_yr: low,
            high_age_yr: high,
            f_low,
            f_high,
        });
        if f_low * f_mid <= 0.0 {
            high = mid;
            f_high = f_mid;
        } else {
            low = mid;
            f_low = f_mid;
        }
        if high - low <= 1e-10 * (1.0 + low.abs() + high.abs()) * 0.5 {
            break;
        }
    }
    ((low + high) * 0.5, trace)
}

fn line_uncertainty(x: f64, cov: Cov2) -> f64 {
    let va = cov[0][0];
    let cov_ab = cov[0][1];
    let vb = cov[1][1];
    (va + 2.0 * x * cov_ab + x * x * vb).max(0.0).sqrt()
}

fn make_intersection(
    age_yr: f64,
    kind: IntersectionKind,
    convention: Convention,
    regression: &RegressionResult,
    root_trace: Vec<RootTrace>,
) -> Intersection {
    let [x, y] = concordia(age_yr, convention);
    let time_derivative =
        residual_derivative(age_yr, convention, regression.intercept, regression.slope);
    let line_one_sigma = line_uncertainty(x, regression.scaled_cov_ab);
    let raw_line_one_sigma = line_uncertainty(x, regression.cov_ab);
    let da = 1.0 / time_derivative;
    let db = x / time_derivative;
    let stable = matches!(kind, IntersectionKind::Stable) && time_derivative.abs() > 1e-12;
    let (sigma_yr, amplification, warning) = if stable {
        let sigma = line_one_sigma / time_derivative.abs();
        (
            Some(sigma),
            Some(1.0 / time_derivative.abs()),
            if regression.scatter_factor > 1.0 {
                Some(format!(
                    "误差已按 MSWD={:.4} 放大 {:.3} 倍",
                    regression.mswd, regression.scatter_factor
                ))
            } else {
                None
            },
        )
    } else {
        (
            None,
            None,
            Some("交点位于或接近切点；时间导数退化，年龄方差不稳定，不能报告有限 1σ".to_string()),
        )
    };
    Intersection {
        age_yr,
        age_ma: age_yr / 1_000_000.0,
        concordia_point: [x, y],
        kind,
        stable,
        sigma_age_yr: sigma_yr,
        sigma_age_ma: sigma_yr.map(|v| v / 1_000_000.0),
        age_intercept_derivative: da,
        age_slope_derivative: db,
        line_one_sigma: if regression.scatter_factor > 1.0 {
            line_one_sigma
        } else {
            raw_line_one_sigma
        },
        time_derivative,
        error_amplification: amplification,
        residual_function: y - (regression.intercept + regression.slope * x),
        root_trace,
        warning,
    }
}

pub fn solve_intersections(
    convention: Convention,
    regression: &RegressionResult,
) -> IntersectionSolution {
    let start = if matches!(convention, Convention::Wetherill) {
        0.0
    } else {
        1.0
    };
    let cells = 6000;
    let mut ages = Vec::with_capacity(cells + 1);
    let mut fs = Vec::with_capacity(cells + 1);
    let mut dfs = Vec::with_capacity(cells + 1);
    for index in 0..=cells {
        let age = start + (MAX_AGE_YR - start) * index as f64 / cells as f64;
        ages.push(age);
        fs.push(residual(
            age,
            convention,
            regression.intercept,
            regression.slope,
        ));
        dfs.push(residual_derivative(
            age,
            convention,
            regression.intercept,
            regression.slope,
        ));
    }

    let mut roots = Vec::new();
    let mut used = vec![false; cells + 1];

    for index in 0..cells {
        if dfs[index].is_finite()
            && dfs[index + 1].is_finite()
            && dfs[index] * dfs[index + 1] <= 0.0
        {
            let (stationary_age, trace) = bisect(
                ages[index],
                ages[index + 1],
                regression.intercept,
                regression.slope,
                convention,
                true,
            );
            let value = residual(
                stationary_age,
                convention,
                regression.intercept,
                regression.slope,
            );
            let scale = 1.0
                + regression.intercept.abs()
                + regression.slope.abs() * concordia(stationary_age, convention)[0].abs();
            if value.abs() <= 5e-8 * scale.max(1.0) {
                roots.push(make_intersection(
                    stationary_age,
                    IntersectionKind::TangentUnstable,
                    convention,
                    regression,
                    trace,
                ));
                used[index] = true;
                used[index + 1] = true;
            }
        }
    }

    for index in 0..cells {
        if used[index] || used[index + 1] {
            continue;
        }
        if fs[index].is_finite() && fs[index + 1].is_finite() && fs[index] * fs[index + 1] < 0.0 {
            let (age, trace) = bisect(
                ages[index],
                ages[index + 1],
                regression.intercept,
                regression.slope,
                convention,
                false,
            );
            roots.push(make_intersection(
                age,
                IntersectionKind::Stable,
                convention,
                regression,
                trace,
            ));
        }
    }

    roots.sort_by(|a, b| a.age_yr.partial_cmp(&b.age_yr).unwrap());
    roots.dedup_by(|a, b| (a.age_yr - b.age_yr).abs() < 2_000_000.0);

    let tangent_count = roots
        .iter()
        .filter(|root| matches!(root.kind, IntersectionKind::TangentUnstable))
        .count();
    let stable_count = roots
        .iter()
        .filter(|root| matches!(root.kind, IntersectionKind::Stable))
        .count();
    let diagnostic = if tangent_count > 0 {
        format!("检测到 {tangent_count} 个退化切点；切点只作为单重不稳定解，不拆成两个人造交点。另有 {stable_count} 个稳定交点。")
    } else if roots.len() == 2 {
        "检测到上下两个稳定交点。".to_string()
    } else if roots.len() < 2 {
        format!("仅检测到 {stable_count} 个稳定交点；请检查年龄范围或数据是否在直线模型内。")
    } else {
        format!("检测到 {stable_count} 个数值交点；该口径下结果超过标准上下交点模型。")
    };

    IntersectionSolution {
        convention,
        intercept: regression.intercept,
        slope: regression.slope,
        intersections: roots,
        diagnostic,
    }
}
