use crate::geometry::{Cov2, PointObservation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionPoint {
    pub id: String,
    pub name: String,
    pub mean: [f64; 2],
    pub cov: Cov2,
    pub adjusted: [f64; 2],
    pub signed_sigma_residual: f64,
    pub chi_square_contribution: f64,
    pub leverage: f64,
    pub influence: f64,
    pub lead_source: String,
    pub lead_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationTrace {
    pub iteration: usize,
    pub intercept: f64,
    pub slope: f64,
    pub profile_chi_square: f64,
    pub bracket_low: f64,
    pub bracket_high: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionResult {
    pub intercept: f64,
    pub slope: f64,
    pub cov_ab: Cov2,
    pub scaled_cov_ab: Cov2,
    pub chi_square: f64,
    pub degrees_freedom: usize,
    pub mswd: f64,
    pub scatter_factor: f64,
    pub iterations: Vec<IterationTrace>,
    pub converged: bool,
    pub initial_intercept: f64,
    pub initial_slope: f64,
    pub points: Vec<RegressionPoint>,
}

struct AdjustedPoint {
    xi: f64,
    yi: f64,
    chi: f64,
    xi_bar: f64,
    yi_bar: f64,
    wi: f64,
}

fn invert(cov: Cov2) -> Cov2 {
    let determinant = cov[0][0] * cov[1][1] - cov[0][1] * cov[0][1];
    [
        [cov[1][1] / determinant, -cov[0][1] / determinant],
        [-cov[1][0] / determinant, cov[0][0] / determinant],
    ]
}

fn adjusted_for_slope(point: &PointObservation, b: f64, a: f64) -> AdjustedPoint {
    let [x, y] = point.mean;
    let precision = invert(point.cov);
    let pxx = precision[0][0];
    let pyy = precision[1][1];
    let pxy = precision[0][1];
    let denominator = pxx + 2.0 * b * pxy + b * b * pyy;
    let xi = x + (pxy + b * pyy) * (y - a - b * x) / denominator;
    let yi = a + b * xi;
    let dx = x - xi;
    let dy = y - yi;
    let chi = dx * (pxx * dx + pxy * dy) + dy * (pxy * dx + pyy * dy);
    AdjustedPoint {
        xi,
        yi,
        chi,
        xi_bar: 0.0,
        yi_bar: 0.0,
        wi: 1.0 / denominator,
    }
}

fn chi_scale(point: &PointObservation, b: f64) -> f64 {
    let vx = point.cov[0][0];
    let vy = point.cov[1][1];
    let cxy = point.cov[0][1];
    vx * b * b - 2.0 * b * cxy + vy
}

fn global_profile(points: &[PointObservation], b: f64) -> (f64, f64, Vec<AdjustedPoint>) {
    let mut a_numerator = 0.0;
    let mut a_denominator = 0.0;

    for point in points {
        let precision = invert(point.cov);
        let pxx = precision[0][0];
        let pyy = precision[1][1];
        let pxy = precision[0][1];
        let denominator = pxx + 2.0 * b * pxy + b * b * pyy;
        let [x, y] = point.mean;
        let u = (pxy + b * pyy) / denominator;
        let v = x + u * (y - b * x);
        let intercept_coefficient = 1.0 - b * u;
        let weight = 1.0 / denominator;
        let a_partial = (y - b * v) * intercept_coefficient;
        a_numerator += weight * a_partial;
        a_denominator += weight * intercept_coefficient * intercept_coefficient;
    }

    let a = a_numerator / a_denominator;
    let mut chi_square = 0.0;
    let mut adjusted = Vec::with_capacity(points.len());
    for point in points {
        let complete = adjusted_for_slope(point, b, a);
        chi_square += complete.chi;
        adjusted.push(complete);
    }

    let sum_w: f64 = adjusted.iter().map(|v| v.wi).sum();
    let xbar: f64 = adjusted.iter().map(|v| v.wi * v.xi).sum::<f64>() / sum_w;
    let ybar: f64 = adjusted.iter().map(|v| v.wi * v.yi).sum::<f64>() / sum_w;
    for item in &mut adjusted {
        item.xi_bar = item.xi - xbar;
        item.yi_bar = item.yi - ybar;
    }
    (chi_square, a, adjusted)
}

pub fn fit_york(points: &[PointObservation]) -> anyhow::Result<RegressionResult> {
    let usable: Vec<_> = points
        .iter()
        .filter(|p| p.valid && p.positive_definite)
        .cloned()
        .collect();
    if usable.len() < 2 {
        anyhow::bail!("至少需要两个正定点才能回归");
    }
    let n = usable.len() as f64;
    let mean_x: f64 = usable.iter().map(|p| p.mean[0]).sum::<f64>() / n;
    let mean_y: f64 = usable.iter().map(|p| p.mean[1]).sum::<f64>() / n;
    let mut initial_slope = usable
        .iter()
        .map(|p| (p.mean[0] - mean_x) * (p.mean[1] - mean_y))
        .sum::<f64>()
        / usable
            .iter()
            .map(|p| (p.mean[0] - mean_x).powi(2))
            .sum::<f64>();
    if !initial_slope.is_finite() {
        initial_slope = 0.0;
    }
    let initial_intercept = mean_y - initial_slope * mean_x;

    let mut low = initial_slope - initial_slope.abs().max(1.0);
    let mut high = initial_slope + initial_slope.abs().max(1.0);
    for _ in 0..60 {
        let fl = global_profile(&usable, low).0;
        let f0 = global_profile(&usable, initial_slope).0;
        let fh = global_profile(&usable, high).0;
        if f0 <= fl && f0 <= fh {
            break;
        }
        if fl < fh {
            low -= (high - low).max(1.0);
        } else {
            high += (high - low).max(1.0);
        }
    }

    let inv_phi = (5.0_f64.sqrt() - 1.0) / 2.0;
    let mut iterations = Vec::new();
    let mut inner_low = low;
    let mut inner_high = high;
    let mut c = high - inv_phi * (high - low);
    let mut d = low + inv_phi * (high - low);
    let mut fc = global_profile(&usable, c).0;
    let mut fd = global_profile(&usable, d).0;
    for iteration in 0..160 {
        let midpoint = (inner_low + inner_high) * 0.5;
        let (_, a_mid, _) = global_profile(&usable, midpoint);
        iterations.push(IterationTrace {
            iteration,
            intercept: a_mid,
            slope: midpoint,
            profile_chi_square: global_profile(&usable, midpoint).0,
            bracket_low: inner_low,
            bracket_high: inner_high,
        });
        if fc < fd {
            inner_high = d;
            d = c;
            fd = fc;
            c = inner_high - inv_phi * (inner_high - inner_low);
            fc = global_profile(&usable, c).0;
        } else {
            inner_low = c;
            c = d;
            fc = fd;
            d = inner_low + inv_phi * (inner_high - inner_low);
            fd = global_profile(&usable, d).0;
        }
        if inner_high - inner_low <= 1e-12 * (1.0 + inner_low.abs() + inner_high.abs()) {
            break;
        }
    }
    let slope = (inner_low + inner_high) * 0.5;
    let (chi_square, intercept, adjusted) = global_profile(&usable, slope);
    let converged = inner_high - inner_low <= 1e-10 * (1.0 + slope.abs());
    let sum_w: f64 = adjusted.iter().map(|v| v.wi).sum();
    let sum_x2: f64 = adjusted.iter().map(|v| v.wi * v.xi_bar * v.xi_bar).sum();
    let var_b = 1.0 / sum_x2.max(1e-300);
    let xbar: f64 = adjusted.iter().map(|v| v.wi * v.xi).sum::<f64>() / sum_w;
    let var_a = 1.0 / sum_w + xbar * xbar * var_b;
    let cov_ab = -xbar * var_b;
    let raw_cov = [[var_a, cov_ab], [cov_ab, var_b]];
    let degrees_freedom = usable.len().saturating_sub(2);
    let mswd = if degrees_freedom == 0 {
        f64::NAN
    } else {
        chi_square / degrees_freedom as f64
    };
    let scatter_factor = if degrees_freedom > 0 && mswd > 1.0 {
        mswd.sqrt()
    } else {
        1.0
    };
    let scaled_cov = raw_cov.map(|row| row.map(|value| value * scatter_factor * scatter_factor));

    let mut diagnostics = Vec::new();
    for (point, adj) in usable.iter().zip(adjusted.iter()) {
        let scale = chi_scale(point, slope);
        let residual = point.mean[1] - intercept - slope * point.mean[0];
        let signed = residual / scale.max(0.0).sqrt();
        let h = if sum_x2 > 0.0 {
            adj.xi_bar * adj.xi_bar / sum_x2 + 1.0 / sum_w
        } else {
            0.0
        };
        diagnostics.push(RegressionPoint {
            id: point.input.id.clone(),
            name: point.input.name.clone(),
            mean: point.mean,
            cov: point.cov,
            adjusted: [adj.xi, adj.yi],
            signed_sigma_residual: signed,
            chi_square_contribution: adj.chi,
            leverage: h,
            influence: signed * h / (1.0 - h.min(0.999_999)).max(1e-300),
            lead_source: point.input.lead_source.clone(),
            lead_note: point.input.lead_note.clone(),
        });
    }

    Ok(RegressionResult {
        intercept,
        slope,
        cov_ab: raw_cov,
        scaled_cov_ab: scaled_cov,
        chi_square,
        degrees_freedom,
        mswd,
        scatter_factor,
        iterations,
        converged,
        initial_intercept,
        initial_slope,
        points: diagnostics,
    })
}
