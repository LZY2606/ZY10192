use concordia_jiaotai::analysis::{analyze, AnalysisRequest, Dataset};
use concordia_jiaotai::db::{AppState, Branch};
use concordia_jiaotai::geometry::Convention;
use std::sync::Arc;

fn dataset() -> Dataset {
    let datasets: Vec<Dataset> = serde_json::from_str(concordia_jiaotai::FIXTURE_JSON).unwrap();
    datasets
        .into_iter()
        .find(|item| item.id == "normal-chord")
        .unwrap()
}

#[test]
fn clear_export_and_reimport_reverifies_runs() {
    let state = Arc::new(AppState::in_memory().unwrap());
    state
        .seed_if_empty(concordia_jiaotai::FIXTURE_JSON)
        .unwrap();
    let data = dataset();
    let request = AnalysisRequest {
        dataset_id: data.id.clone(),
        source_convention: Convention::Wetherill,
        target_convention: Convention::Wetherill,
        included_point_ids: data.points.iter().map(|point| point.id.clone()).collect(),
        scatter_model: "analytical_or_mswd".into(),
    };
    let response = analyze(request.clone(), &data).unwrap();
    let branch = state
        .create_branch(Branch {
            id: String::new(),
            dataset_id: data.id.clone(),
            name: "全部点".into(),
            parent_id: None,
            included_point_ids: request.included_point_ids.clone(),
            note: "test".into(),
        })
        .unwrap();
    state
        .save_run(&request, &response, Some(branch.id.clone()))
        .unwrap();
    let snapshot = state.export_snapshot().unwrap();
    assert_eq!(snapshot.datasets.len(), 4);
    assert_eq!(snapshot.runs.len(), 1);
    state.clear_all().unwrap();
    assert!(state.list_datasets().unwrap().is_empty());
    let report = state.import_snapshot(&snapshot).unwrap();
    assert_eq!(report.datasets, 4);
    assert_eq!(report.branches, 1);
    assert_eq!(report.runs_verified, 1);
    assert_eq!(
        state.list_runs().unwrap()[0].fingerprint,
        response.run_fingerprint
    );
}
