use concordia_jiaotai::analysis::{analyze, AnalysisRequest, Dataset};
use concordia_jiaotai::geometry::Convention;

fn datasets() -> Vec<Dataset> {
    serde_json::from_str(concordia_jiaotai::FIXTURE_JSON).unwrap()
}
fn dataset(id: &str) -> Dataset {
    datasets().into_iter().find(|item| item.id == id).unwrap()
}
fn all_ids(data: &Dataset) -> Vec<String> {
    data.points.iter().map(|point| point.id.clone()).collect()
}
fn request(id: &str, source: Convention, target: Convention) -> AnalysisRequest {
    let data = dataset(id);
    AnalysisRequest {
        dataset_id: id.into(),
        source_convention: source,
        target_convention: target,
        included_point_ids: all_ids(&data),
        scatter_model: "analytical_or_mswd".into(),
    }
}

#[test]
fn normal_chord_recovers_two_stable_intersections() {
    let data = dataset("normal-chord");
    let result = analyze(
        request("normal-chord", Convention::Wetherill, Convention::Wetherill),
        &data,
    )
    .unwrap();
    let regression = result.regression.as_ref().unwrap();
    assert!(regression.converged);
    assert!((regression.slope - 0.08474104844).abs() < 1e-10);
    assert!((regression.intercept - 0.01306252879).abs() < 1e-10);
    let solution = result.intersections.as_ref().unwrap();
    let stable: Vec<_> = solution
        .intersections
        .iter()
        .filter(|root| root.stable)
        .collect();
    assert_eq!(stable.len(), 2);
    assert!((stable[0].age_ma - 200.0).abs() < 1.0);
    assert!((stable[1].age_ma - 1200.0).abs() < 1.0);
    assert!(stable[0].sigma_age_ma.is_some());
}

#[test]
fn invalid_rho_blocks_ellipse_and_fit_until_excluded() {
    let data = dataset("validation-mix");
    let mut req = request(
        "validation-mix",
        Convention::Wetherill,
        Convention::Wetherill,
    );
    let result = analyze(req.clone(), &data).unwrap();
    let bad = result.points.iter().find(|point| point.id == "v2").unwrap();
    assert!(!bad.valid);
    assert!(bad.ellipse.is_none());
    assert!(result.regression.is_none());
    req.included_point_ids.retain(|id| id == "v1");
    let only_valid = analyze(req, &data).unwrap();
    assert!(only_valid.regression.is_none());
    assert!(only_valid.warning.unwrap().contains("至少需要两个"));
}

#[test]
fn wetherill_tangent_is_single_unstable_solution() {
    let data = dataset("wetherill-tangent");
    let result = analyze(
        request(
            "wetherill-tangent",
            Convention::Wetherill,
            Convention::Wetherill,
        ),
        &data,
    )
    .unwrap();
    let solution = result.intersections.as_ref().unwrap();
    let tangents: Vec<_> = solution
        .intersections
        .iter()
        .filter(|root| {
            root.kind == concordia_jiaotai::intersection::IntersectionKind::TangentUnstable
        })
        .collect();
    assert_eq!(tangents.len(), 1);
    assert!((tangents[0].age_ma - 1000.0).abs() < 1.0);
    assert!(tangents[0].sigma_age_ma.is_none());
    assert!(!solution.intersections.iter().any(|root| root.stable));
}

#[test]
fn tera_wasserburg_tangent_does_not_split_into_pair() {
    let data = dataset("tera-wasserburg-tangent");
    let result = analyze(
        request(
            "tera-wasserburg-tangent",
            Convention::TeraWasserburg,
            Convention::TeraWasserburg,
        ),
        &data,
    )
    .unwrap();
    let solution = result.intersections.as_ref().unwrap();
    let tangents: Vec<_> = solution
        .intersections
        .iter()
        .filter(|root| {
            root.kind == concordia_jiaotai::intersection::IntersectionKind::TangentUnstable
        })
        .collect();
    assert_eq!(tangents.len(), 1);
    assert!((tangents[0].age_ma - 1000.0).abs() < 1.0);
    assert!(tangents[0].sigma_age_ma.is_none());
}

#[test]
fn switching_coordinates_preserves_fixture_data_and_fingerprint_changes() {
    let data = dataset("normal-chord");
    let w = analyze(
        request("normal-chord", Convention::Wetherill, Convention::Wetherill),
        &data,
    )
    .unwrap();
    let tw = analyze(
        request(
            "normal-chord",
            Convention::Wetherill,
            Convention::TeraWasserburg,
        ),
        &data,
    )
    .unwrap();
    assert_eq!(w.points.len(), tw.points.len());
    assert!(tw
        .points
        .iter()
        .all(|point| point.positive_definite && point.ellipse.is_some()));
    assert_ne!(w.run_fingerprint, tw.run_fingerprint);
}

#[test]
fn invalid_point_remains_diagnostic_through_coordinate_switch() {
    let data = dataset("validation-mix");
    let result = analyze(
        request(
            "validation-mix",
            Convention::Wetherill,
            Convention::TeraWasserburg,
        ),
        &data,
    )
    .unwrap();
    let bad = result.points.iter().find(|point| point.id == "v2").unwrap();
    assert!(!bad.valid);
    assert!(bad.ellipse.is_none());
    assert!(bad.errors.iter().any(|message| message.contains("rho")));
}

#[test]
fn york_fit_handles_scattered_correlated_points_and_mswd_amplification() {
    use concordia_jiaotai::geometry::{PointInput, PointObservation};
    use concordia_jiaotai::regression::fit_york;

    let inputs = [
        (0.30, 0.040_0, 0.030, 0.003_5, 0.35),
        (0.80, 0.079_0, 0.025, 0.003_2, -0.20),
        (1.40, 0.133_0, 0.030, 0.004_0, 0.25),
        (2.00, 0.180_0, 0.035, 0.004_5, -0.30),
    ];
    let points = inputs
        .iter()
        .enumerate()
        .map(|(index, (x, y, sx, sy, rho))| {
            PointObservation::from_input(
                PointInput {
                    id: format!("s{index}"),
                    name: format!("S{index}"),
                    x: *x,
                    y: *y,
                    sx: *sx,
                    sy: *sy,
                    rho: *rho,
                    lead_source: "test lead".into(),
                    lead_note: String::new(),
                },
                Convention::Wetherill,
            )
        })
        .collect::<Vec<_>>();
    let result = fit_york(&points).unwrap();
    assert!(result.converged);
    assert!((result.slope - 0.082).abs() < 0.015);
    assert!(result.mswd.is_finite());
    assert!(result.scatter_factor >= 1.0);
    assert!(result
        .points
        .iter()
        .all(|point| point.signed_sigma_residual.is_finite()));
}
