use crate::constants::*;
use crate::model::{Covariance2, IntersectionAge, RegressionFit, RootTrace, RootTraceStep};

#[derive(Debug, Clone)]
struct LineRoot {
    age: f64,
    trace: RootTrace,
}

fn concordia_residual(age_years: f64, fit: &RegressionFit) -> f64 {
    let x5 = age_x5(age_years);
    let x8 = age_x8(age_years);
    x8 - fit.intercept - fit.slope * x5
}

fn concordia_tangent_slope(age_years: f64) -> f64 {
    LAMBDA_238_PER_YEAR / LAMBDA_235_PER_YEAR * (1.0 + age_x8(age_years))
        / (1.0 + age_x5(age_years))
}

fn age_x5(age_years: f64) -> f64 {
    (LAMBDA_235_PER_YEAR * age_years).exp() - 1.0
}

fn age_x8(age_years: f64) -> f64 {
    (LAMBDA_238_PER_YEAR * age_years).exp() - 1.0
}

fn residual_derivative(age_years: f64, fit: &RegressionFit) -> f64 {
    LAMBDA_238_PER_YEAR * (1.0 + age_x8(age_years))
        - fit.slope * LAMBDA_235_PER_YEAR * (1.0 + age_x5(age_years))
}

fn bisect_root(initial_lower: f64, initial_upper: f64, fit: &RegressionFit) -> Option<LineRoot> {
    let mut lower = initial_lower;
    let mut upper = initial_upper;
    let mut f_lower = concordia_residual(lower, fit);
    let mut f_upper = concordia_residual(upper, fit);
    if !f_lower.is_finite() || !f_upper.is_finite() || f_lower * f_upper > 0.0 {
        return None;
    }
    let mut path = Vec::new();
    let mut converged = false;
    for iteration in 0..100 {
        path.push(RootTraceStep {
            iteration,
            lower_age_years: lower,
            upper_age_years: upper,
            f_lower,
            f_upper,
        });
        let midpoint = (lower + upper) * 0.5;
        let f_mid = concordia_residual(midpoint, fit);
        if f_lower * f_mid <= 0.0 {
            upper = midpoint;
            f_upper = f_mid;
        } else {
            lower = midpoint;
            f_lower = f_mid;
        }
        if upper - lower < 100.0 {
            converged = true;
            break;
        }
    }
    let age = (lower + upper) * 0.5;
    Some(LineRoot {
        age,
        trace: RootTrace {
            initial_lower_age_years: initial_lower,
            initial_upper_age_years: initial_upper,
            converged,
            iterations: path.len(),
            path,
        },
    })
}

fn bisect_stationary(
    initial_lower: f64,
    initial_upper: f64,
    fit: &RegressionFit,
) -> Option<(f64, f64, RootTrace)> {
    let mut lower = initial_lower;
    let mut upper = initial_upper;
    let mut f_lower = residual_derivative(lower, fit);
    let mut f_upper = residual_derivative(upper, fit);
    if f_lower * f_upper > 0.0 {
        return None;
    }
    let mut path = Vec::new();
    let mut converged = false;
    for iteration in 0..100 {
        path.push(RootTraceStep {
            iteration,
            lower_age_years: lower,
            upper_age_years: upper,
            f_lower,
            f_upper,
        });
        let midpoint = (lower + upper) * 0.5;
        let f_mid = residual_derivative(midpoint, fit);
        if f_lower * f_mid <= 0.0 {
            upper = midpoint;
            f_upper = f_mid;
        } else {
            lower = midpoint;
            f_lower = f_mid;
        }
        if upper - lower < 100.0 {
            converged = true;
            break;
        }
    }
    let age = (lower + upper) * 0.5;
    Some((
        age,
        concordia_residual(age, fit),
        RootTrace {
            initial_lower_age_years: initial_lower,
            initial_upper_age_years: initial_upper,
            converged,
            iterations: path.len(),
            path,
        },
    ))
}

fn propagated_uncertainty(age: f64, fit: &RegressionFit) -> Option<(f64, f64)> {
    let x5 = age_x5(age);
    let ft = residual_derivative(age, fit);
    if ft.abs() < 1.0e-20 {
        return None;
    }
    let covariance: &Covariance2 = &fit.covariance;
    let gradient_a = -1.0;
    let gradient_b = -x5;
    let line_sigma_square = gradient_a * gradient_a * covariance.xx
        + 2.0 * gradient_a * gradient_b * covariance.xy
        + gradient_b * gradient_b * covariance.yy;
    if line_sigma_square <= 0.0 || !line_sigma_square.is_finite() {
        return None;
    }
    let sigma_years = line_sigma_square.sqrt() / ft.abs();
    let amplification = LAMBDA_238_PER_YEAR / ft.abs();
    Some((sigma_years, amplification))
}

fn ordinary_age(root: LineRoot, fit: &RegressionFit, label: &str, kind: &str) -> IntersectionAge {
    let (one_sigma, amplification) = propagated_uncertainty(root.age, fit)
        .map(|(sigma, amplification)| (Some(sigma), Some(amplification)))
        .unwrap_or((None, None));
    IntersectionAge {
        label: label.to_string(),
        age_years: root.age,
        one_sigma_years: one_sigma,
        stable: one_sigma
            .map(|sigma| sigma.is_finite() && sigma < 500.0e6)
            .unwrap_or(false),
        multiplicity: 1,
        kind: kind.to_string(),
        error_amplification: amplification,
        trace: root.trace,
    }
}

fn tangent_age(age: f64, trace: RootTrace, fit: &RegressionFit) -> IntersectionAge {
    let local_slope = concordia_tangent_slope(age);
    let slope_mismatch = fit.slope - local_slope;
    let _ = slope_mismatch;
    IntersectionAge {
        label: "切触点（单重不稳定解）".to_string(),
        age_years: age,
        one_sigma_years: None,
        stable: false,
        multiplicity: 2,
        kind: "tangent_double_root".to_string(),
        error_amplification: Some(f64::INFINITY),
        trace,
    }
}

pub fn intersection_ages(fit: &RegressionFit) -> Vec<IntersectionAge> {
    let mut values = Vec::with_capacity(ROOT_GRID_COUNT + 1);
    for index in 0..=ROOT_GRID_COUNT {
        let age = MAX_AGE_YEARS * index as f64 / ROOT_GRID_COUNT as f64;
        values.push((age, concordia_residual(age, fit)));
    }

    let mut stationary = Vec::new();
    for pair in values.windows(3) {
        let derivative_left = (pair[1].1 - pair[0].1) / (pair[1].0 - pair[0].0);
        let derivative_right = (pair[2].1 - pair[1].1) / (pair[2].0 - pair[1].0);
        if derivative_left * derivative_right < 0.0 {
            if let Some(found) = bisect_stationary(pair[0].0, pair[2].0, fit) {
                stationary.push(found);
            }
        }
    }

    if let Some((age, _value, trace)) = stationary
        .iter()
        .find(|(_, value, _)| value.abs() < TANGENT_VALUE_TOLERANCE)
    {
        return vec![tangent_age(*age, trace.clone(), fit)];
    }

    let mut roots = Vec::new();
    for pair in values.windows(2) {
        if pair[0].1 * pair[1].1 <= 0.0 {
            if let Some(root) = bisect_root(pair[0].0, pair[1].0, fit) {
                roots.push(root);
            }
        }
    }
    roots.sort_by(|left, right| left.age.total_cmp(&right.age));

    let near_tangent_stationary = stationary
        .iter()
        .find(|(_, value, _)| value.abs() < NEAR_TANGENT_VALUE_TOLERANCE);
    if roots.len() == 2 {
        let gap = roots[1].age - roots[0].age;
        if gap < TANGENT_AGE_GAP_YEARS {
            if let Some((age, _, trace)) = near_tangent_stationary {
                return vec![tangent_age(*age, trace.clone(), fit)];
            }
        }
    }

    let mut result = Vec::new();
    for (index, root) in roots.into_iter().enumerate() {
        let label = if index == 0 {
            "下交点年龄"
        } else {
            "上交点年龄"
        };
        let kind = if index == 0 {
            "lower_intersection"
        } else {
            "upper_intersection"
        };
        result.push(ordinary_age(root, fit, label, kind));
    }
    result
}
