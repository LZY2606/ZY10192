use concordia_crossing::analysis::{analyze_input, fixed_runs};
use concordia_crossing::geometry::{diagnose_input_point, tw_to_wetherill, wetherill_to_tw};
use concordia_crossing::model::{Convention, CoordinatePoint, Covariance2, InputPoint, LeadSource};

fn tw_point() -> CoordinatePoint {
    CoordinatePoint {
        x: 13.0,
        y: 0.051,
        covariance: Covariance2::new(0.032, -0.000_041, 0.000_002_8),
    }
}

#[test]
fn tw_wetherill_covariance_round_trips() {
    let original = tw_point();
    let (wetherill, _) = tw_to_wetherill(original).unwrap();
    let (returned, _) = wetherill_to_tw(wetherill).unwrap();
    assert!(wetherill.covariance.is_positive_definite());
    assert!(returned.covariance.is_positive_definite());
    approx::assert_relative_eq!(returned.x, original.x, epsilon = 1e-11);
    approx::assert_relative_eq!(returned.y, original.y, epsilon = 1e-13);
    approx::assert_relative_eq!(
        returned.covariance.xx,
        original.covariance.xx,
        epsilon = 1e-11
    );
    approx::assert_relative_eq!(
        returned.covariance.xy,
        original.covariance.xy,
        epsilon = 1e-13
    );
    approx::assert_relative_eq!(
        returned.covariance.yy,
        original.covariance.yy,
        epsilon = 1e-15
    );
}

#[test]
fn correlations_must_be_within_closed_interval() {
    let point = InputPoint {
        id: "bad".into(),
        label: None,
        convention: Convention::TeraWasserburg,
        x: 13.0,
        y: 0.05,
        sigma_x: 0.1,
        sigma_y: 0.001,
        correlation: 1.000_1,
        common_lead: None,
    };
    let diagnostic = diagnose_input_point(&point);
    assert!(!diagnostic.valid);
    assert!(!diagnostic.ellipse_available);
    assert!(diagnostic.errors[0].contains("闭区间"));
}

#[test]
fn fixed_tangent_fixture_is_one_unstable_double_root() {
    let input = fixed_runs()
        .into_iter()
        .find(|run| run.id.as_deref() == Some("fixture-tangent"))
        .unwrap();
    let result = analyze_input(input);
    let intersections = &result.baseline.intersections;
    assert_eq!(intersections.len(), 1);
    assert_eq!(intersections[0].multiplicity, 2);
    assert!(!intersections[0].stable);
    assert_eq!(intersections[0].kind, "tangent_double_root");
    approx::assert_relative_eq!(intersections[0].age_years, 1.0e9, epsilon = 3.0e6);
    assert!(intersections[0]
        .error_amplification
        .expect("amplification")
        .is_infinite());
}

#[test]
fn normal_fixture_has_two_stable_intersections() {
    let input = fixed_runs()
        .into_iter()
        .find(|run| run.id.as_deref() == Some("fixture-normal"))
        .unwrap();
    let result = analyze_input(input);
    assert_eq!(result.baseline.intersections.len(), 2);
    let lower = &result.baseline.intersections[0];
    let upper = &result.baseline.intersections[1];
    assert!(lower.stable);
    assert!(upper.stable);
    approx::assert_relative_eq!(lower.age_years, 100.0e6, epsilon = 15.0e6);
    approx::assert_relative_eq!(upper.age_years, 1.2e9, epsilon = 30.0e6);
}

#[test]
fn endpoint_correlation_values_are_singular_not_ellipses() {
    let point = InputPoint {
        id: "rho-minus-one".into(),
        label: None,
        convention: Convention::Wetherill,
        x: 2.0,
        y: 0.16,
        sigma_x: 0.01,
        sigma_y: 0.001,
        correlation: -1.0,
        common_lead: Some(concordia_crossing::model::CommonLead {
            source: LeadSource::None,
            reference: "test".into(),
            pb207_pb206: None,
        }),
    };
    let diagnostic = diagnose_input_point(&point);
    assert!(!diagnostic.ellipse_available);
    assert!(diagnostic
        .errors
        .iter()
        .any(|error| error.contains("非正定")));
}
