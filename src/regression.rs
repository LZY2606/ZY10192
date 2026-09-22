use crate::geometry::symmetric_eigen;
use crate::model::{
    CoordinatePoint, Covariance2, EigenMode, RegressionFit, RegressionTrace, ResidualRow, TraceStep,
};

#[derive(Debug, Clone, Copy)]
struct Evaluation {
    theta: f64,
    rho: f64,
    objective: f64,
    chi_square: f64,
}

fn normal_variance(point: &CoordinatePoint, theta: f64) -> f64 {
    let (sin, cos) = theta.sin_cos();
    point.covariance.xx * sin * sin
        + 2.0 * point.covariance.xy * sin * cos
        + point.covariance.yy * cos * cos
}

fn normal_distance(point: &CoordinatePoint, theta: f64, rho: f64) -> f64 {
    point.x * theta.sin() + point.y * theta.cos() - rho
}

fn evaluate(points: &[CoordinatePoint], theta: f64) -> Evaluation {
    let mut weighted_sum = 0.0;
    let mut weight_sum = 0.0;
    let mut chi_square = 0.0;
    for point in points {
        let weight = 1.0 / normal_variance(point, theta);
        let projection = point.x * theta.sin() + point.y * theta.cos();
        weighted_sum += weight * projection;
        weight_sum += weight;
    }
    let rho = weighted_sum / weight_sum;
    for point in points {
        let residual = normal_distance(point, theta, rho);
        chi_square += residual * residual / normal_variance(point, theta);
    }
    Evaluation {
        theta,
        rho,
        objective: chi_square,
        chi_square,
    }
}

fn ordinary_least_squares(points: &[CoordinatePoint]) -> (f64, f64) {
    let count = points.len() as f64;
    let mean_x = points.iter().map(|point| point.x).sum::<f64>() / count;
    let mean_y = points.iter().map(|point| point.y).sum::<f64>() / count;
    let mut sxx = 0.0;
    let mut sxy = 0.0;
    for point in points {
        let dx = point.x - mean_x;
        let dy = point.y - mean_y;
        sxx += dx * dx;
        sxy += dx * dy;
    }
    let slope = if sxx > f64::MIN_POSITIVE {
        sxy / sxx
    } else {
        0.0
    };
    let intercept = mean_y - slope * mean_x;
    let theta = if sxx > f64::MIN_POSITIVE {
        -slope.atan()
    } else {
        0.0
    };
    (theta, intercept)
}

fn optimize(points: &[CoordinatePoint]) -> (Evaluation, RegressionTrace) {
    let (initial_theta, initial_intercept) = ordinary_least_squares(points);
    let initial = evaluate(points, initial_theta);
    let mut path = vec![TraceStep {
        iteration: 0,
        theta_rad: initial.theta,
        intercept_normal: initial.rho,
        objective: initial.objective,
        update: 0.0,
    }];

    let mut best = initial;
    let mut left = -std::f64::consts::FRAC_PI_2;
    let mut right = std::f64::consts::FRAC_PI_2;
    let scans = 144;
    for index in 0..=scans {
        let theta = left + (right - left) * index as f64 / scans as f64;
        let candidate = evaluate(points, theta);
        if candidate.objective < best.objective {
            best = candidate;
        }
    }
    let scan_span = std::f64::consts::PI / scans as f64;
    left = (best.theta - scan_span).max(-std::f64::consts::FRAC_PI_2);
    right = (best.theta + scan_span).min(std::f64::consts::FRAC_PI_2);

    let golden = (3.0 - 5.0_f64.sqrt()) * 0.5;
    let mut inner_left = left + golden * (right - left);
    let mut inner_right = right - golden * (right - left);
    let mut eval_left = evaluate(points, inner_left);
    let mut eval_right = evaluate(points, inner_right);

    for iteration in 1..=80 {
        let previous = best;
        if eval_left.objective < eval_right.objective {
            right = inner_right;
            inner_right = inner_left;
            eval_right = eval_left;
            inner_left = left + golden * (right - left);
            eval_left = evaluate(points, inner_left);
        } else {
            left = inner_left;
            inner_left = inner_right;
            eval_left = eval_right;
            inner_right = right - golden * (right - left);
            eval_right = evaluate(points, inner_right);
        }
        best = if eval_left.objective < eval_right.objective {
            eval_left
        } else {
            eval_right
        };
        let update = best.theta - previous.theta;
        path.push(TraceStep {
            iteration,
            theta_rad: best.theta,
            intercept_normal: best.rho,
            objective: best.objective,
            update,
        });
        if (right - left).abs() < 1.0e-13 && update.abs() < 1.0e-12 {
            break;
        }
    }

    let initial_rho = initial_intercept * initial_theta.cos();
    let trace = RegressionTrace {
        method: "ordinary-least-squares-start;global-scan;golden-section-GLS".to_string(),
        initial_theta_rad: initial_theta,
        initial_intercept_normal: initial_rho,
        converged: (right - left).abs() < 1.0e-10,
        iterations: path.len().saturating_sub(1),
        path,
    };
    (best, trace)
}

fn inverse_2(a: f64, b: f64, c: f64) -> Option<([[f64; 2]; 2], f64)> {
    let determinant = a * c - b * b;
    if determinant.abs() <= f64::MIN_POSITIVE || !determinant.is_finite() {
        return None;
    }
    Some((
        [
            [c / determinant, -b / determinant],
            [-b / determinant, a / determinant],
        ],
        determinant,
    ))
}

fn parameter_covariance(
    points: &[CoordinatePoint],
    theta: f64,
    rho: f64,
) -> Option<(Covariance2, f64)> {
    let mut h_theta_theta = 0.0;
    let mut h_theta_rho = 0.0;
    let mut h_rho_rho = 0.0;
    for point in points {
        let (sin, cos) = theta.sin_cos();
        let x = point.x;
        let y = point.y;
        let qxx = point.covariance.xx;
        let qxy = point.covariance.xy;
        let qyy = point.covariance.yy;
        let w = normal_variance(point, theta);
        let residual = x * sin + y * cos - rho;
        let w_theta = 2.0 * (qxx - qyy) * sin * cos + 2.0 * qxy * (cos * cos - sin * sin);
        let q_theta = x * cos - y * sin;
        let g_theta = q_theta - 0.5 * residual * w_theta / w;
        let g_rho = -1.0;
        h_theta_theta += g_theta * g_theta / w;
        h_theta_rho += g_theta * g_rho / w;
        h_rho_rho += g_rho * g_rho / w;
    }
    let (inverse, determinant) = inverse_2(h_theta_theta, h_theta_rho, h_rho_rho)?;
    let d_slope_theta = -1.0 / (theta.cos() * theta.cos());
    let d_intercept_rho = 1.0 / theta.cos();
    let d_intercept_theta = rho * theta.sin() / (theta.cos() * theta.cos());
    let xx = d_slope_theta * d_slope_theta * inverse[0][0];
    let xy = d_slope_theta * (d_intercept_theta * inverse[0][0] + d_intercept_rho * inverse[0][1]);
    let yy = d_intercept_theta * d_intercept_theta * inverse[0][0]
        + 2.0 * d_intercept_theta * d_intercept_rho * inverse[0][1]
        + d_intercept_rho * d_intercept_rho * inverse[1][1];
    Some((Covariance2::new(xx, xy, yy), determinant))
}

fn cos_safe(theta: f64) -> f64 {
    theta.cos().max(1.0e-8)
}

pub fn fit_line(points: &[CoordinatePoint]) -> Option<(RegressionFit, RegressionTrace)> {
    if points.len() < 2 {
        return None;
    }
    let (best, trace) = optimize(points);
    let (param_covariance, information_determinant) =
        parameter_covariance(points, best.theta, best.rho)?;
    let (major, minor) = symmetric_eigen(&param_covariance);
    let condition_number = major.eigenvalue / minor.eigenvalue.max(f64::MIN_POSITIVE);
    let degrees_of_freedom = points.len().saturating_sub(2);
    let mswd = if degrees_of_freedom == 0 {
        f64::NAN
    } else {
        best.chi_square / degrees_of_freedom as f64
    };
    let fit = RegressionFit {
        slope: -best.theta.tan(),
        intercept: best.rho / cos_safe(best.theta),
        theta_rad: best.theta,
        normal_intercept: best.rho,
        covariance: param_covariance,
        mswd,
        chi_square: best.chi_square,
        degrees_of_freedom,
        condition_number,
        major_mode: major,
        minor_mode: minor,
    };
    let _ = information_determinant;
    Some((fit, trace))
}

pub fn residual_rows(
    labeled_points: &[(String, String, CoordinatePoint)],
    fit: &RegressionFit,
    excluded: &[String],
) -> Vec<ResidualRow> {
    let theta = fit.theta_rad;
    let rho = fit.normal_intercept;
    let mut rows = Vec::new();
    for (id, label, point) in labeled_points {
        if excluded.iter().any(|excluded_id| excluded_id == id) {
            continue;
        }
        let variance = normal_variance(point, theta);
        let distance = normal_distance(point, theta, rho);
        let mut h_theta_theta = 0.0;
        let mut h_theta_rho = 0.0;
        let mut h_rho_rho = 0.0;
        for (other_id, _, other) in labeled_points {
            if excluded.iter().any(|excluded_id| excluded_id == other_id) {
                continue;
            }
            let other_variance = normal_variance(other, theta);
            let (sin, cos) = theta.sin_cos();
            let residual = normal_distance(other, theta, rho);
            let w_theta = 2.0 * (other.covariance.xx - other.covariance.yy) * sin * cos
                + 2.0 * other.covariance.xy * (cos * cos - sin * sin);
            let g_theta =
                (other.x * cos - other.y * sin) - 0.5 * residual * w_theta / other_variance;
            h_theta_theta += g_theta * g_theta / other_variance;
            h_theta_rho -= g_theta / other_variance;
            h_rho_rho += 1.0 / other_variance;
        }
        let fisher_determinant = h_theta_theta * h_rho_rho - h_theta_rho * h_theta_rho;
        let (sin, cos) = theta.sin_cos();
        let residual = distance;
        let w_theta = 2.0 * (point.covariance.xx - point.covariance.yy) * sin * cos
            + 2.0 * point.covariance.xy * (cos * cos - sin * sin);
        let g_theta = (point.x * cos - point.y * sin) - 0.5 * residual * w_theta / variance;
        let g = [g_theta, -1.0];
        let fisher_inverse = [
            [
                h_rho_rho / fisher_determinant,
                -h_theta_rho / fisher_determinant,
            ],
            [
                -h_theta_rho / fisher_determinant,
                h_theta_theta / fisher_determinant,
            ],
        ];
        let weight = 1.0 / variance;
        let leverage = weight
            * (g[0] * fisher_inverse[0][0] * g[0]
                + 2.0 * g[0] * fisher_inverse[0][1] * g[1]
                + g[1] * fisher_inverse[1][1] * g[1]);
        let normalized = distance / variance.sqrt();
        let cooks = normalized * normalized * leverage / (2.0 * (1.0 - leverage).max(1.0e-9));
        rows.push(ResidualRow {
            point_id: id.clone(),
            label: label.clone(),
            normalized_residual: normalized,
            signed_distance: distance,
            residual_variance: variance,
            leverage,
            cooks_distance: cooks,
            delta_lower_age_years: None,
            delta_upper_age_years: None,
        });
    }
    rows
}

pub fn eigenmodes(covariance: &Covariance2) -> (EigenMode, EigenMode) {
    symmetric_eigen(covariance)
}
