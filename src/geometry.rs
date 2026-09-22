use crate::constants::U238_OVER_U235;
use crate::model::{
    CommonLead, Convention, CoordinatePoint, Covariance2, EigenMode, InputPoint, LeadSource,
    PointDiagnostic,
};

fn default_common_lead(convention: Convention) -> CommonLead {
    let source = match convention {
        Convention::Wetherill => LeadSource::None,
        Convention::TeraWasserburg => LeadSource::UncorrectedCommonLead,
    };
    CommonLead {
        source,
        reference: "fixture-or-input".to_string(),
        pb207_pb206: None,
    }
}

fn covariance_from_input(point: &InputPoint) -> Result<Covariance2, String> {
    if !(-1.0..=1.0).contains(&point.correlation) {
        return Err(format!("相关系数 {} 不在闭区间 [-1, 1]", point.correlation));
    }
    if !point.sigma_x.is_finite() || point.sigma_x <= 0.0 {
        return Err("x 标准差必须为有限正数".to_string());
    }
    if !point.sigma_y.is_finite() || point.sigma_y <= 0.0 {
        return Err("y 标准差必须为有限正数".to_string());
    }
    let covariance = Covariance2::new(
        point.sigma_x * point.sigma_x,
        point.correlation * point.sigma_x * point.sigma_y,
        point.sigma_y * point.sigma_y,
    );
    if !covariance.is_positive_definite() {
        return Err("输入协方差非正定矩阵".to_string());
    }
    Ok(covariance)
}

fn transform_covariance(j: [[f64; 2]; 2], covariance: &Covariance2) -> Covariance2 {
    let s = covariance;
    let xx = j[0][0] * j[0][0] * s.xx + 2.0 * j[0][0] * j[0][1] * s.xy + j[0][1] * j[0][1] * s.yy;
    let xy = j[0][0] * j[1][0] * s.xx
        + (j[0][0] * j[1][1] + j[0][1] * j[1][0]) * s.xy
        + j[0][1] * j[1][1] * s.yy;
    let yy = j[1][0] * j[1][0] * s.xx + 2.0 * j[1][0] * j[1][1] * s.xy + j[1][1] * j[1][1] * s.yy;
    Covariance2::new(xx, xy, yy)
}

pub fn tw_to_wetherill(point: CoordinatePoint) -> Result<(CoordinatePoint, [[f64; 2]; 2]), String> {
    let (u, v) = (point.x, point.y);
    if !u.is_finite() || !v.is_finite() || u == 0.0 {
        return Err("Tera-Wasserburg x=238U/206Pb 必须为非零有限值".to_string());
    }
    let x = U238_OVER_U235 * v / u;
    let y = 1.0 / u;
    if x <= -1.0 || y <= -1.0 {
        return Err("变换后的 Wetherill 比值必须大于 -1".to_string());
    }
    let jacobian = [[-x / u, U238_OVER_U235 / u], [-y / u, 0.0]];
    let covariance = transform_covariance(jacobian, &point.covariance);
    if !covariance.is_positive_definite() {
        return Err("Tera-Wasserburg 转 Wetherill 后协方差非正定，停止生成椭圆".to_string());
    }
    Ok((CoordinatePoint { x, y, covariance }, jacobian))
}

pub fn wetherill_to_tw(point: CoordinatePoint) -> Result<(CoordinatePoint, [[f64; 2]; 2]), String> {
    let (x, y) = (point.x, point.y);
    if !x.is_finite() || !y.is_finite() || y == 0.0 {
        return Err("Wetherill y=206Pb/238U 必须为非零有限值".to_string());
    }
    let u = 1.0 / y;
    let v = x * u / U238_OVER_U235;
    let jacobian = [[0.0, -u / y], [u / U238_OVER_U235, -v / y]];
    let covariance = transform_covariance(jacobian, &point.covariance);
    if !covariance.is_positive_definite() {
        return Err("Wetherill 转 Tera-Wasserburg 后协方差非正定，停止生成椭圆".to_string());
    }
    Ok((
        CoordinatePoint {
            x: u,
            y: v,
            covariance,
        },
        jacobian,
    ))
}

pub fn discordance_percent(wetherill: &CoordinatePoint) -> Option<f64> {
    if wetherill.x <= -1.0 || wetherill.y <= -1.0 {
        return None;
    }
    let age207_235 = wetherill.x.ln_1p() / crate::constants::LAMBDA_235_PER_YEAR;
    let age206_238 = wetherill.y.ln_1p() / crate::constants::LAMBDA_238_PER_YEAR;
    if age207_235 <= 0.0 {
        return None;
    }
    Some(100.0 * (1.0 - age206_238 / age207_235))
}

pub fn diagnose_input_point(point: &InputPoint) -> PointDiagnostic {
    let mut errors = Vec::new();
    let common_lead = point
        .common_lead
        .clone()
        .unwrap_or_else(|| default_common_lead(point.convention));
    let covariance = match covariance_from_input(point) {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(error);
            None
        }
    };

    let original = CoordinatePoint {
        x: point.x,
        y: point.y,
        covariance: covariance.unwrap_or(Covariance2::new(f64::NAN, f64::NAN, f64::NAN)),
    };
    let (wetherill, tw) = match point.convention {
        Convention::Wetherill => {
            let w = if covariance.is_some() {
                Some(original)
            } else {
                None
            };
            let tw = w.and_then(|value| wetherill_to_tw(value).ok());
            match (w, tw) {
                (Some(w_value), Some(tw_value)) => (w_value, tw_value.0),
                _ => {
                    errors.push("坐标或协方差无法在两套口径之间回转".to_string());
                    (original, original)
                }
            }
        }
        Convention::TeraWasserburg => {
            let tw_input = original;
            let converted = covariance.and_then(|cov| {
                tw_to_wetherill(CoordinatePoint {
                    x: point.x,
                    y: point.y,
                    covariance: cov,
                })
                .ok()
            });
            match converted {
                Some((w_value, _)) => {
                    let tw_back = wetherill_to_tw(w_value).ok().map(|(value, _)| value);
                    match tw_back {
                        Some(tw_value) => (w_value, tw_value),
                        None => {
                            errors
                                .push("Wetherill 回转 Tera-Wasserburg 后协方差非正定".to_string());
                            (w_value, tw_input)
                        }
                    }
                }
                None => {
                    errors.push("Tera-Wasserburg 转 Wetherill 失败，停止生成椭圆".to_string());
                    (original, tw_input)
                }
            }
        }
    };

    if !point.x.is_finite() || !point.y.is_finite() {
        errors.push("点坐标必须为有限值".to_string());
    }
    let valid = errors.is_empty();
    let discordance_percent = if valid {
        discordance_percent(&wetherill)
    } else {
        None
    };

    PointDiagnostic {
        id: point.id.clone(),
        label: point.label.clone().unwrap_or_else(|| point.id.clone()),
        included: valid,
        valid,
        errors,
        wetherill,
        tera_wasserburg: tw,
        common_lead,
        discordance_percent,
        ellipse_available: valid,
    }
}

pub fn symmetric_eigen(covariance: &Covariance2) -> (EigenMode, EigenMode) {
    let half_trace = (covariance.xx + covariance.yy) * 0.5;
    let delta = 0.5
        * ((covariance.xx - covariance.yy).powi(2) + 4.0 * covariance.xy * covariance.xy).sqrt();
    let major_value = half_trace + delta;
    let minor_value = half_trace - delta;
    let major_dx = covariance.xy;
    let major_dy = major_value - covariance.xx;
    let norm = (major_dx * major_dx + major_dy * major_dy)
        .sqrt()
        .max(1.0e-300);
    let major = EigenMode {
        eigenvalue: major_value,
        x: major_dx / norm,
        y: major_dy / norm,
    };
    let minor = EigenMode {
        eigenvalue: minor_value,
        x: -major.y,
        y: major.x,
    };
    (major, minor)
}
