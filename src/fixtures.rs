use crate::constants::{LAMBDA_235_PER_YEAR, LAMBDA_238_PER_YEAR};
use crate::model::{CommonLead, Convention, InputPoint, LeadSource, RunInput};

fn common_lead(source: LeadSource, reference: &str) -> CommonLead {
    CommonLead {
        source,
        pb207_pb206: None,
        reference: reference.to_string(),
    }
}

fn corrected_point(
    id: &str,
    label: &str,
    x: f64,
    y: f64,
    sigma_x: f64,
    sigma_y: f64,
    correlation: f64,
    source: LeadSource,
    reference: &str,
) -> InputPoint {
    InputPoint {
        id: id.to_string(),
        label: Some(label.to_string()),
        convention: Convention::Wetherill,
        x,
        y,
        sigma_x,
        sigma_y,
        correlation,
        common_lead: Some(common_lead(source, reference)),
    }
}

fn exp_m1(lambda: f64, age_years: f64) -> f64 {
    (lambda * age_years).exp() - 1.0
}

fn normal_fixture() -> RunInput {
    let lower_age = 100.0e6;
    let upper_age = 1_200.0e6;
    let x_lower = exp_m1(LAMBDA_235_PER_YEAR, lower_age);
    let y_lower = exp_m1(LAMBDA_238_PER_YEAR, lower_age);
    let x_upper = exp_m1(LAMBDA_235_PER_YEAR, upper_age);
    let y_upper = exp_m1(LAMBDA_238_PER_YEAR, upper_age);
    let slope = (y_upper - y_lower) / (x_upper - x_lower);
    let intercept = y_lower - slope * x_lower;
    let fractions = [0.15_f64, 0.40, 0.62, 0.88];
    let settings = [
        (
            "N-01",
            "正常椭圆 A",
            0.003_8,
            0.000_72,
            -0.22,
            LeadSource::Measured204,
            "204Pb measured",
        ),
        (
            "N-02",
            "正常椭圆 B",
            0.003_2,
            0.000_61,
            0.35,
            LeadSource::StaceyKramers1975,
            "Stacey-Kramers 1975",
        ),
        (
            "N-03",
            "正常椭圆 C",
            0.004_5,
            0.000_88,
            0.78,
            LeadSource::ProceduralBlank,
            "laboratory blank",
        ),
        (
            "N-04",
            "正常椭圆 D",
            0.003_6,
            0.000_68,
            -0.55,
            LeadSource::ModelCommonLead,
            "model initial Pb",
        ),
    ];
    let points = settings
        .into_iter()
        .zip(fractions)
        .map(|((id, label, sx, sy, corr, source, reference), fraction)| {
            let x = x_lower + fraction * (x_upper - x_lower);
            let y = slope * x + intercept;
            corrected_point(id, label, x, y, sx, sy, corr, source, reference)
        })
        .collect();
    RunInput {
        id: Some("fixture-normal".to_string()),
        name: "固定 fixture：正常不一致线".to_string(),
        points,
    }
}

fn invalid_correlation_fixture() -> RunInput {
    RunInput {
        id: Some("fixture-invalid-correlation".to_string()),
        name: "固定 fixture：相关系数越界".to_string(),
        points: vec![InputPoint {
            id: "BAD-RHO".to_string(),
            label: Some("rho=1.04".to_string()),
            convention: Convention::TeraWasserburg,
            x: 14.7,
            y: 0.052,
            sigma_x: 0.28,
            sigma_y: 0.001_8,
            correlation: 1.04,
            common_lead: Some(common_lead(
                LeadSource::UncorrectedCommonLead,
                "retained common Pb; covariance deliberately invalid",
            )),
        }],
    }
}

fn tangent_fixture() -> RunInput {
    let age = 1_000.0e6;
    let x_tangent = exp_m1(LAMBDA_235_PER_YEAR, age);
    let y_tangent = exp_m1(LAMBDA_238_PER_YEAR, age);
    let slope = LAMBDA_238_PER_YEAR / LAMBDA_235_PER_YEAR * (1.0 + y_tangent) / (1.0 + x_tangent);
    let intercept = y_tangent - slope * x_tangent;
    let offsets = [-0.58_f64, -0.08, 0.46];
    let settings = [
        (
            "T-01",
            "切触点 A",
            0.004_2,
            0.000_39,
            0.18,
            LeadSource::Measured204,
        ),
        (
            "T-02",
            "切触点 B",
            0.003_7,
            0.000_34,
            -0.41,
            LeadSource::StaceyKramers1975,
        ),
        (
            "T-03",
            "切触点 C",
            0.005_1,
            0.000_47,
            0.62,
            LeadSource::ProceduralBlank,
        ),
    ];
    let points = settings
        .into_iter()
        .zip(offsets)
        .map(|((id, label, sx, sy, corr, source), offset)| {
            corrected_point(
                id,
                label,
                x_tangent + offset,
                slope * (x_tangent + offset) + intercept,
                sx,
                sy,
                corr,
                source,
                "tangent-line common lead audit",
            )
        })
        .collect();
    RunInput {
        id: Some("fixture-tangent".to_string()),
        name: "固定 fixture：协和曲线切触".to_string(),
        points,
    }
}

pub fn fixtures() -> Vec<RunInput> {
    vec![
        normal_fixture(),
        invalid_correlation_fixture(),
        tangent_fixture(),
    ]
}
