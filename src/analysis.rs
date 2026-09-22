use crate::fixtures;
use crate::geometry::diagnose_input_point;
use crate::model::*;
use crate::regression::{fit_line, residual_rows};
use crate::solve::intersection_ages;
use sha2::{Digest, Sha256};

pub fn canonical_fingerprint(input: &RunInput) -> String {
    let run = without_id(input);
    let fingerprint_payload = serde_json::json!({
        "equation_version": crate::constants::EQUATION_VERSION,
        "constants": equation_constants(),
        "run": run,
    });
    let value = serde_json::to_value(fingerprint_payload).expect("fingerprint payload serializes");
    canonical_json(&value)
}

fn without_id(input: &RunInput) -> RunInput {
    RunInput {
        id: None,
        name: input.name.clone(),
        points: input.points.clone(),
    }
}

fn canonical_json(value: &serde_json::Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_json_inner(value).as_bytes());
    format!("{:x}", hasher.finalize())
}

fn canonical_json_inner(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Array(values) => {
            let joined = values
                .iter()
                .map(canonical_json_inner)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{joined}]")
        }
        serde_json::Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            let joined = keys
                .iter()
                .map(|key| format!("{}:{}", key, canonical_json_inner(&map[*key])))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{joined}}}")
        }
    }
}

pub fn stable_run_id(input: &RunInput) -> String {
    canonical_fingerprint(&without_id(input))
        .chars()
        .take(16)
        .collect()
}

pub fn analyze_input(mut input: RunInput) -> RunResult {
    let stable_id = stable_run_id(&input);
    input.id = Some(input.id.clone().unwrap_or(stable_id));
    let diagnostics: Vec<PointDiagnostic> = input.points.iter().map(diagnose_input_point).collect();
    let fingerprint = canonical_fingerprint(&input);
    let baseline = solve_branch("全有效点基线", &input, &diagnostics, Vec::new());
    RunResult {
        id: input.id.clone().unwrap(),
        name: input.name,
        fingerprint,
        equation_version: crate::constants::EQUATION_VERSION.to_string(),
        constants: equation_constants(),
        points: diagnostics,
        baseline,
    }
}

pub fn solve_branch(
    name: &str,
    input: &RunInput,
    diagnostics: &[PointDiagnostic],
    excluded: Vec<String>,
) -> BranchResult {
    let mut warnings = Vec::new();
    let invalid: Vec<_> = diagnostics
        .iter()
        .filter(|point| !point.valid)
        .map(|point| point.id.clone())
        .collect();
    for id in &invalid {
        if !excluded.contains(id) {
            warnings.push(format!("点 {id} 未通过校验，不参与回归且不生成椭圆"));
        }
    }
    let included_points: Vec<_> = input
        .points
        .iter()
        .zip(diagnostics)
        .filter(|(input_point, diagnostic)| diagnostic.valid && !excluded.contains(&input_point.id))
        .map(|(input_point, diagnostic)| {
            (
                input_point.id.clone(),
                diagnostic.label.clone(),
                diagnostic.wetherill,
            )
        })
        .collect();

    if included_points.len() < 2 {
        return BranchResult {
            name: name.to_string(),
            excluded_point_ids: excluded,
            included_count: included_points.len(),
            fit: None,
            trace: None,
            intersections: Vec::new(),
            residual_rows: Vec::new(),
            warnings: [
                warnings,
                vec!["有效点不足两个，无法拟合 Wetherill 不一致线".to_string()],
            ]
            .concat(),
        };
    }

    let coordinates: Vec<_> = included_points.iter().map(|(_, _, point)| *point).collect();
    let (fit, trace) =
        fit_line(&coordinates).expect("two or more positive-definite points can fit");
    let intersections = intersection_ages(&fit);
    if intersections.len() == 1 && intersections[0].kind == "tangent_double_root" {
        warnings
            .push("检测到协和曲线切触点：报告一个双重、不稳定解，不拆成上下两个交点".to_string());
    }
    if fit.condition_number > 1.0e10 {
        warnings.push(format!(
            "线参数协方差条件数 {:.3e} 表明误差放大接近奇异",
            fit.condition_number
        ));
    }
    let mut residual_rows = residual_rows(&included_points, &fit, &[]);
    enrich_influence(
        input,
        diagnostics,
        &excluded,
        &intersections,
        &mut residual_rows,
    );

    BranchResult {
        name: name.to_string(),
        excluded_point_ids: excluded,
        included_count: included_points.len(),
        fit: Some(fit),
        trace: Some(trace),
        intersections,
        residual_rows,
        warnings,
    }
}

fn enrich_influence(
    input: &RunInput,
    diagnostics: &[PointDiagnostic],
    excluded: &[String],
    baseline_intersections: &[IntersectionAge],
    rows: &mut [ResidualRow],
) {
    for row in rows.iter_mut() {
        let mut one_excluded = excluded.to_vec();
        one_excluded.push(row.point_id.clone());
        let branch = solve_branch("leave-one-out", input, diagnostics, one_excluded);
        row.delta_lower_age_years = compare_age(
            baseline_intersections,
            &branch.intersections,
            "lower_intersection",
            "tangent_double_root",
        );
        row.delta_upper_age_years = compare_age(
            baseline_intersections,
            &branch.intersections,
            "upper_intersection",
            "tangent_double_root",
        );
    }
}

fn compare_age(
    baseline: &[IntersectionAge],
    branch: &[IntersectionAge],
    ordinary_kind: &str,
    tangent_kind: &str,
) -> Option<f64> {
    let baseline_age = if baseline.len() == 1 && baseline[0].kind == tangent_kind {
        Some(baseline[0].age_years)
    } else {
        baseline
            .iter()
            .find(|age| age.kind == ordinary_kind)
            .map(|age| age.age_years)
    }?;
    let branch_age = if branch.len() == 1 && branch[0].kind == tangent_kind {
        Some(branch[0].age_years)
    } else {
        branch
            .iter()
            .find(|age| age.kind == ordinary_kind)
            .map(|age| age.age_years)
    }?;
    Some(branch_age - baseline_age)
}

pub fn fixed_runs() -> Vec<RunInput> {
    fixtures::fixtures()
}
