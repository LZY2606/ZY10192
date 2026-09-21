# 协和线交台

本地 Rust + Axum + SQLite U-Pb 协和线交点分析服务。页面保留每个点误差椭圆的方向，可在 Wetherill 与 Tera-Wasserburg 口径间转换均值与协方差，执行相关误差的 York 直线回归，并求协和曲线上下交点。

## 安装与演示

```bash
cargo fetch --locked
cargo test --locked
cargo run --locked -- --listen 127.0.0.1:5532
```

浏览器访问 <http://127.0.0.1:5532>，页面标题为“协和线交台”。

默认 SQLite 文件为 `data/concordia.sqlite`，首次启动自动创建并导入内置夹具。也可以指定：

```bash
cargo run --locked -- --listen 127.0.0.1:5532 --database /tmp/concordia.sqlite
```

## 数据口径

固定常数进入运行指纹，不从当前时间或外网读取：

- λ₂₃₈ = `1.55125e-10 yr⁻¹`
- λ₂₃₅ = `9.8485e-10 yr⁻¹`
- ²³⁸U/²³⁵U = `137.818`
- 常数版本：`fixed-upb-constants-v1`
- 方程版本：`wetherill-tw-v1`

Wetherill 坐标：

- x = ²⁰⁷Pb/²³⁵U = `exp(λ235 t) - 1`
- y = ²⁰⁶Pb/²³⁸U = `exp(λ238 t) - 1`

Tera-Wasserburg 坐标：

- X = ²³⁸U/²⁰⁶Pb = `1 / y`
- Y = ²⁰⁷Pb/²⁰⁶Pb = `x / (137.818 y)`

从 Wetherill 转换到 Tera-Wasserburg 时使用解析 Jacobian：

```text
J = [[0, -1/y²],
     [1/(137.818 y), -x/(137.818 y²)]]
Cov_TW = J Cov_W Jᵀ
```

回转使用逆 Jacobian。相关系数必须在闭区间 `[-1, 1]`；若输入或变换后的协方差不是正定矩阵，该点保留原始诊断但不生成椭圆、不参与回归。

## 固定夹具

`fixtures/fixtures.json` 内置四个数据集：

- `normal-chord`：4 个正常方向误差椭圆，共线于约 200 Ma 与 1200 Ma 的协和交线。
- `validation-mix`：1 个正常点和 1 个 `rho = 1.34` 的越界点；越界点不画椭圆。
- `wetherill-tangent`：精确位于 Wetherill 1000 Ma 协和曲线切线上，返回一个 `tangent_unstable` 解。
- `tera-wasserburg-tangent`：精确位于 Tera-Wasserburg 1000 Ma 切线上，不把数值上的近邻双根误报为两个稳定交点。

每个点带有共同铅校正来源字段 `lead_source` / `lead_note`。切点年龄的时间导数退化，因此只输出单重不稳定诊断，不报告有限 1σ 年龄，也不会人为拆成两个稳定交点。

## 页面能力

- 切换绘图与回归口径；原始夹具口径保持只读。
- 绘制 95% 误差椭圆，椭圆方向由完整协方差特征值和特征角决定。
- 点表显示均值、标准差、相关系数、符号 σ 残差、χ² 贡献、杠杆和影响。
- 显示共同铅校正来源。
- 通过勾选点建立包含/排除分支并保存运行。
- 两个分支可按点级残差、χ² 贡献和影响并排比较。
- 求解结果包含初始截距/斜率、Golden-section 收敛路径、MSWD、交点年龄、年龄不确定度和误差放大系数。
- “清空并恢复夹具”删除当前数据库内容并重新导入固定夹具。
- “导出运行记录”生成完整 JSON 快照；清空数据库后可通过“导入复核”重新导入。导入时会重新计算分析并核对 SHA-256 运行指纹。

## API

- `GET /api/constants`：固定常数与方程版本。
- `GET /api/fixture`：内置夹具原文。
- `GET /api/datasets`：数据集列表。
- `POST /api/analyze`：提交 `AnalysisRequest` 并返回点、椭圆、协方差、回归、路径和交点。
- `GET/POST /api/branches/{dataset_id}`：列分支或创建分支。
- `GET /api/runs`：已保存运行。
- `GET /api/snapshot`：导出数据集、分支和运行。
- `POST /api/snapshot`：清空、导入并复核快照。
- `POST /api/reset`：清空数据库并恢复内置夹具。

`POST /api/analyze` 请求示例：

```json
{
  "save_run": true,
  "request": {
    "dataset_id": "normal-chord",
    "source_convention": "wetherill",
    "target_convention": "wetherill",
    "included_point_ids": ["n1", "n2", "n3", "n4"],
    "scatter_model": "analytical_or_mswd"
  }
}
```

## 自动化测试

```bash
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

测试覆盖：

- 相关系数闭区间和非正定椭圆拒绝。
- Wetherill / Tera-Wasserburg 均值与协方差传播及回转。
- 正常夹具恢复约 200 Ma、1200 Ma 两个稳定交点。
- Wetherill 与 Tera-Wasserburg 切点均返回一个不稳定单解。
- HTTP 首页、固定常数 API 和切点分析 API。
- SQLite 快照导出、清空、导入和运行指纹复核。
