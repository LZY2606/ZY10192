# 协和线交台

本地 Rust + Axum + SQLite + 离线 Web UI。它处理 U-Pb 成对比值、标准差、相关系数和共同铅校正来源，显式在 Wetherill 与 Tera-Wasserburg 口径之间转换坐标和协方差，并诊断 Wetherill 不一致线与协和曲线的上下交点。

## 安装与演示

```bash
cargo fetch --locked
cargo test --locked
cargo run --locked -- --listen 127.0.0.1:5532
```

打开 <http://127.0.0.1:5532>，页面标题和主标题均为“协和线交台”。默认数据库文件是当前目录的 `concordia.db`，首次启动会写入固定 fixture；也可用：

```bash
cargo run --locked -- --listen 127.0.0.1:5532 --database ./data/run.db
cargo run --locked -- --no-seed
```

## 数据口径

每个点输入：

- `convention`: `wetherill` 或 `tera_wasserburg`。
- Wetherill：`x=207Pb/235U`，`y=206Pb/238U`。
- Tera-Wasserburg：`x=238U/206Pb=U`，`y=207Pb/206Pb=V`。
- `sigma_x`、`sigma_y` 为 1σ 标准差，`correlation` 必须属于闭区间 `[-1,1]`。
- `common_lead.source` 与 `reference` 记录共同铅校正来源，作为审计元数据展示；未校正 Tera-Wasserburg 输入保留 `uncorrected_common_lead`。

坐标转换使用固定 `238U/235U=137.88`：

```text
X = 137.88 V / U
Y = 1/U
U = 1/Y
V = X / (137.88 Y)
```

协方差按显式 Jacobian 传播，而不是重画误差棒。例如 Tera-Wasserburg 到 Wetherill：

```text
J = [ -X/U      137.88/U ]
    [ -Y/U      0         ]
Cov(W) = J Cov(TW) J^T
```

逆变换同样保存 Jacobian，测试会把 TW 转 W 再转回 TW，核对坐标和协方差一致。相关系数越界、标准差非正、或任一次变换后的协方差非正定，点会保留在记录中，但不参与回归，也不生成误差椭圆。

## 年龄方程与运行指纹

Wetherill 协和曲线：

```text
X(t)=exp(λ235 t)-1
Y(t)=exp(λ238 t)-1
```

固定常数写入运行指纹，不从系统当前时间或外网读取：

- `λ235 = 9.8485e-10 / year`
- `λ238 = 1.55125e-10 / year`
- `238U/235U = 137.88`
- 指纹版本：`upb-wetherill-tw-constants-v1`

不一致线写成 `Y=a+bX`，用每个点的完整 2×2 协方差做广义最小二乘。拟合保留 OLS 初始值、全局扫描和黄金分割收敛路径。年龄 1σ 由线参数协方差经残差梯度传播：

```text
F(t)=Y(t)-a-bX(t)
σ_t² = (∇line F)^T Cov(a,b) (∇line F) / (dF/dt)²
```

切触时 `dF/dt=0`，上下交点合并为一个多重根。求解器先定位残差静止点，当静止点残差在固定容差内为零时报告 `tangent_double_root`：一个年龄、`multiplicity=2`、不稳定、误差放大为无穷。它不会人为生成两个很接近的交点。

## 固定 fixture

启动或点击“恢复固定 fixture”会得到：

- `fixture-normal`：四个正的 2σ 误差椭圆，沿 100 Ma–1200 Ma 的普通不一致线，包含不同相关系数与共同铅来源。
- `fixture-invalid-correlation`：一个 Tera-Wasserburg 点，`correlation=1.04`，用于验证闭区间校验和椭圆阻断。
- `fixture-tangent`：三个正定点，拟合线在 1000 Ma 与协和曲线相切；验收应看到一个单重不稳定解，而不是上下两个人造交点。

## 页面操作

- 顶部切换 Wetherill / Tera-Wasserburg；TW 图的协和曲线由同一组常数从 Wetherill 数值生成。
- 点击点可查看两套坐标、2×2 协方差、共同铅来源、发散度和校验错误。
- 点击“建立排除分支”把当前点加入排除集；“重置分支”回到全部有效点基线。
- 表格并排显示归一残差、正常方向残差方差、杠杆、Cook 距离，以及留一删除导致的上下交点年龄变化。
- 侧边栏展示初始值、迭代路径尾部、交点二分轨迹、1σ 和误差放大模式。

## 导出、清空与复核

- “导出运行记录”下载 JSON，只包含输入和固定方程版本，不保存易变时钟。
- “清空数据库”删除 SQLite 中全部运行。
- “导入复核”选择导出的 JSON，系统重新校验、传播协方差、回归并计算指纹。
- HTTP 自动化测试覆盖：导出 → 清空 → 导入后 3 条记录恢复，且切触 fixture 仍为 `tangent_double_root`。

## API 摘要

- `GET /api/runs`：列出并实时计算所有运行。
- `POST /api/runs`：导入一条运行输入。
- `GET /api/runs/{id}`：查看运行结果。
- `DELETE /api/runs/{id}`：删除一条记录。
- `POST /api/runs/{id}/branches`：请求体 `{"excluded_point_ids":["T-01"]}`。
- `GET /api/export` / `POST /api/import`：批量记录导出导入。
- `POST /api/runs/clear`：清空记录。
- `POST /api/fixtures/reset`：恢复固定 fixture。

## 测试

`cargo test --locked` 包含 8 个自动化测试：

- TW→W→TW 坐标和协方差回转。
- 相关系数越界拒绝生成椭圆。
- `ρ=±1` 的半正定协方差不生成面积椭圆。
- 正常 fixture 的两个稳定上下交点。
- 切触 fixture 的单个双重不稳定根。
- 首页标题。
- 清空后导入复核。
- 排除点分支 API。
