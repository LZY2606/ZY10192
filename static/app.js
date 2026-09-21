let datasets = [];
let currentDataset = null;
let currentAnalysis = null;
let branches = [];
let compareResponses = {};
const $ = (id) => document.getElementById(id);

async function api(url, options = {}) {
  const response = await fetch(url, { headers: { 'Content-Type': 'application/json' }, ...options });
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || `HTTP ${response.status}`);
  return data;
}
function fmt(value, digits = 4) { return Number.isFinite(value) ? value.toFixed(digits) : '—'; }
function ageText(root) {
  const sigma = root.sigma_age_ma == null ? '不稳定' : `±${fmt(root.sigma_age_ma, 3)} Ma`;
  return `${root.kind === 'tangent_unstable' ? '切点 ' : ''}${fmt(root.age_ma, 4)} Ma ${sigma}`;
}
async function loadDatasets() {
  datasets = (await api('/api/datasets')).datasets;
  $('datasetSelect').innerHTML = datasets.map(d => `<option value="${d.id}">${d.name}</option>`).join('');
  await selectDataset();
}
async function selectDataset() {
  const id = $('datasetSelect').value;
  currentDataset = datasets.find(d => d.id === id);
  const wetherill = ['normal-chord','validation-mix','wetherill-tangent'].includes(id);
  $('sourceConventionText').dataset.value = wetherill ? 'wetherill' : 'tera_wasserburg';
  $('targetConvention').value = wetherill ? 'wetherill' : 'tera_wasserburg';
  await loadBranches();
  await runAnalysis(false);
}
function includedIds() {
  return [...document.querySelectorAll('.includePoint:checked')].map(x => x.value);
}
function makeRequest() {
  return {
    dataset_id: currentDataset.id,
    source_convention: $('sourceConventionText').dataset.value,
    target_convention: $('targetConvention').value,
    included_point_ids: includedIds(),
    scatter_model: 'analytical_or_mswd'
  };
}
async function runAnalysis(save = true, branchId = null) {
  if (!currentDataset) return;
  const selected = includedIds();
  const request = {
    dataset_id: currentDataset.id,
    source_convention: $('sourceConventionText').dataset.value,
    target_convention: $('targetConvention').value,
    included_point_ids: selected.length ? selected : currentDataset.points.map(p => p.id),
    scatter_model: 'analytical_or_mswd'
  };
  const payload = await api('/api/analyze', { method: 'POST', body: JSON.stringify({ request, save_run: save, branch_id: branchId }) });
  currentAnalysis = payload.analysis;
  renderPoints(currentAnalysis);
  drawPlot(currentAnalysis);
  renderSummary(currentAnalysis, payload.run_id);
  if (save) loadRuns();
}
function renderPoints(analysis) {
  const rows = analysis.points.map(p => {
    const r = p.regression_point;
    return `<tr class="${p.valid ? '' : 'bad'}">
      <td><label class="check"><input class="includePoint" type="checkbox" value="${p.id}" ${p.included ? 'checked' : ''}> ${p.name}</label><br><span class="small">${p.lead_source}</span></td>
      <td>${fmt(p.mean[0],5)}</td><td>${fmt(p.mean[1],6)}</td><td>${fmt(p.sx,5)}</td><td>${fmt(p.sy,6)}</td><td>${fmt(p.rho,3)}</td>
      <td>${r ? fmt(r.signed_sigma_residual,2) : '—'}</td><td>${r ? fmt(r.chi_square_contribution,3) : '—'}</td><td>${r ? fmt(r.leverage,3) : '—'}</td><td>${r ? fmt(r.influence,3) : '—'}</td>
      <td>${p.valid ? '<span class="good">有效</span>' : p.errors.join('<br>')}</td></tr>`;
  }).join('');
  $('pointTable').innerHTML = `<table><thead><tr><th>点/共同铅来源</th><th>x</th><th>y</th><th>σx</th><th>σy</th><th>ρ</th><th>σ残差</th><th>χ²贡献</th><th>杠杆</th><th>影响</th><th>诊断</th></tr></thead><tbody>${rows}</tbody></table>`;
  document.querySelectorAll('.includePoint').forEach(x => x.addEventListener('change', () => runAnalysis(false)));
}
function bounds(analysis) {
  let valuesX = [], valuesY = [];
  analysis.points.forEach(p => { if (p.valid) { valuesX.push(p.mean[0]); valuesY.push(p.mean[1]); }});
  if (!valuesX.length) { valuesX=[0,1]; valuesY=[0,1]; }
  let minX=Math.min(...valuesX), maxX=Math.max(...valuesX), minY=Math.min(...valuesY), maxY=Math.max(...valuesY);
  const padX=(maxX-minX || .1)*.18, padY=(maxY-minY || .02)*.25;
  if (analysis.regression) { minX=Math.min(minX,0); }
  return {minX:minX-padX,maxX:maxX+padX,minY:Math.min(0,minY-padY),maxY:maxY+padY};
}
function drawPlot(analysis) {
  const canvas=$('plot'), ctx=canvas.getContext('2d'), b=bounds(analysis), W=canvas.width,H=canvas.height,m=54;
  const X=x=>m+(x-b.minX)/(b.maxX-b.minX)*(W-2*m), Y=y=>H-m-(y-b.minY)/(b.maxY-b.minY)*(H-2*m);
  ctx.clearRect(0,0,W,H); ctx.fillStyle='white';ctx.fillRect(0,0,W,H);
  ctx.strokeStyle='#d5dde4';ctx.fillStyle='#495867';ctx.lineWidth=1;ctx.font='12px sans-serif';
  for(let i=0;i<=8;i++){const x=m+i*(W-2*m)/8,y=H-m-i*(H-2*m)/8;ctx.beginPath();ctx.moveTo(x,H-m);ctx.lineTo(x,m);ctx.stroke();ctx.beginPath();ctx.moveTo(m,y);ctx.lineTo(W-m,y);ctx.stroke();}
  ctx.strokeStyle='#788b9b';ctx.beginPath();ctx.moveTo(m,H-m);ctx.lineTo(W-m,H-m);ctx.moveTo(m,H-m);ctx.lineTo(m,m);ctx.stroke();
  const target=analysis.request.target_convention;
  const maxAge=target==='wetherill'?4.5e9:4.5e9, points=500;
  ctx.strokeStyle='#1e6f9f';ctx.lineWidth=3;ctx.beginPath();
  let entered=false;
  for(let i=0;i<=points;i++){ const t=maxAge*i/points; const p=window.__concordia(t,target); if(p[0]<b.minX||p[0]>b.maxX||p[1]<b.minY||p[1]>b.maxY){entered=false;continue;} const x=X(p[0]),y=Y(p[1]); if(!entered){ctx.moveTo(x,y);entered=true}else ctx.lineTo(x,y); }
  ctx.stroke();
  if(analysis.regression){const reg=analysis.regression;ctx.strokeStyle='#d1495b';ctx.lineWidth=2;ctx.beginPath();ctx.moveTo(X(b.minX),Y(reg.intercept+reg.slope*b.minX));ctx.lineTo(X(b.maxX),Y(reg.intercept+reg.slope*b.maxX));ctx.stroke();}
  analysis.points.forEach((p,i)=>{
    ctx.strokeStyle=p.valid?'#2a9d8f':'#c1121f';ctx.fillStyle=p.valid?'#2a9d8f':'#c1121f';ctx.lineWidth=1.5;
    if(p.valid && p.positive_definite && p.ellipse){ const e=p.ellipse,s=Math.sqrt(5.99146),rx=Math.abs(e.semi_major_sigma*s*(X(b.minX+1)-m)),ry=Math.abs(e.semi_major_sigma*s*(H-m-Y(b.minY+1))); ctx.save();ctx.translate(X(p.mean[0]),Y(p.mean[1]));ctx.rotate(-e.angle_rad);ctx.beginPath();ctx.ellipse(0,0,rx,ry,0,0,Math.PI*2);ctx.stroke();ctx.restore(); }
    ctx.beginPath();ctx.arc(X(p.mean[0]),Y(p.mean[1]),p.valid?4:6,0,Math.PI*2);ctx.fill();
    ctx.fillStyle='#17202a';ctx.fillText(p.name,X(p.mean[0])+6,Y(p.mean[1])-6);
  });
  if(analysis.intersections) analysis.intersections.intersections.forEach(root=>{ctx.fillStyle=root.kind==='tangent_unstable'?'#9d4edd':'#003566';ctx.beginPath();ctx.arc(X(root.concordia_point[0]),Y(root.concordia_point[1]),7,0,Math.PI*2);ctx.fill();ctx.fillText(ageText(root),X(root.concordia_point[0])+8,Y(root.concordia_point[1])+4);});
  ctx.fillStyle='#17202a';ctx.fillText(target==='wetherill'?'²⁰⁷Pb/²³⁵U':'²³⁸U/²⁰⁶Pb',W/2-30,H-14);ctx.save();ctx.translate(18,H/2);ctx.rotate(-Math.PI/2);ctx.fillText(target==='wetherill'?'²⁰⁶Pb/²³⁸U':'²⁰⁷Pb/²⁰⁶Pb',-40,0);ctx.restore();
  $('diagnostic').textContent=analysis.intersections?.diagnostic || analysis.warning || '';
}
window.__concordia=(t,target)=>{const l8=1.55125e-10,l7=9.8485e-10,R=137.818,e7=Math.exp(l7*t),e8=Math.exp(l8*t),x7=e7-1,x8=e8-1;return target==='wetherill'?[x7,x8]:[1/x8,x7/(R*x8)];};
function renderSummary(analysis, runId) {
  const reg=analysis.regression, sol=analysis.intersections;
  const text={constants:analysis.constants, run_fingerprint:analysis.run_fingerprint, run_id:runId, request:analysis.request,
    regression:reg && {intercept:reg.intercept,slope:reg.slope,cov_ab:reg.scaled_cov_ab,initial:[reg.initial_intercept,reg.initial_slope],chi_square:reg.chi_square,degrees_freedom:reg.degrees_freedom,mswd:reg.mswd,scatter_factor:reg.scatter_factor,converged:reg.converged,iterations:reg.iterations.length,path:reg.iterations},
    intersections:sol};
  $('summary').textContent=JSON.stringify(text,null,2);
}
async function loadBranches() {
  if (!currentDataset) return;
  branches=(await api(`/api/branches/${currentDataset.id}`)).branches;
  const options=branches.map(b=>`<option value="${b.id}">${b.name}</option>`).join('');
  $('branches').innerHTML=branches.map(b=>`<div class="branch"><button data-load="${b.id}">${b.name}</button><span class="small">${b.included_point_ids.join(', ')}</span><button data-run="${b.id}">保存运行</button></div>`).join('') || '<span class="small">尚无分支</span>';
  $('compareA').innerHTML=options; $('compareB').innerHTML=options;
  document.querySelectorAll('[data-load]').forEach(btn=>btn.onclick=()=>loadBranch(btn.dataset.load));
  document.querySelectorAll('[data-run]').forEach(btn=>btn.onclick=()=>loadBranch(btn.getAttribute('data-run'),true,btn.getAttribute('data-run')));
}
function loadBranch(id,save=false,branchId=null) { const b=branches.find(x=>x.id===id); document.querySelectorAll('.includePoint').forEach(cb=>cb.checked=b.included_point_ids.includes(cb.value)); runAnalysis(save,branchId); }
async function branchRequest(branch) { return { dataset_id:currentDataset.id, source_convention:$('sourceConventionText').dataset.value, target_convention:$('targetConvention').value, included_point_ids:branch.included_point_ids, scatter_model:'analytical_or_mswd' }; }
async function compareBranches(){ const ids=[$('compareA').value,$('compareB').value].filter(Boolean); if(ids.length<2)return; const selected=branches.filter(b=>ids.includes(b.id)); compareResponses={}; for(const b of selected){ const request=await branchRequest(b); compareResponses[b.id]=(await api('/api/analyze',{method:'POST',body:JSON.stringify({request,save_run:false})})).analysis; } renderComparison(selected); }
function renderComparison(selected){ const ids=selected.map(b=>b.id); const byId=Object.fromEntries(currentDataset.points.map(p=>[p.id,p.name])); const rows=Object.keys(byId).map(id=>`<tr><td>${byId[id]}</td>${ids.map(bid=>{const p=compareResponses[bid].points.find(x=>x.id===id),r=p?.regression_point;return `<td>${p?.included?'包含':'排除'}<br>${r?fmt(r.signed_sigma_residual,3):'—'}<br>${r?fmt(r.chi_square_contribution,3):'—'}<br>${r?fmt(r.influence,3):'—'}</td>`}).join('')}</tr>`).join(''); $('comparison').innerHTML=`<table><thead><tr><th>点</th>${selected.map(b=>`<th>${b.name}<br><span class="small">包含/σ残差/χ²/影响</span></th>`).join('')}</tr></thead><tbody>${rows}</tbody></table>`; }
async function saveBranch(){ const name=$('branchName').value || '未命名分支'; const branch={id:'',dataset_id:currentDataset.id,name,parent_id:null,included_point_ids:includedIds(),note:'页面创建'}; await api(`/api/branches/${currentDataset.id}`,{method:'POST',body:JSON.stringify(branch)}); await loadBranches(); }
async function loadRuns(){ $('runs').textContent=JSON.stringify((await api('/api/runs')).runs.map(r=>({id:r.id,fingerprint:r.fingerprint,request:r.request,ages:r.response.intersections?.intersections.map(ageText)})),null,2); }
async function reset(){ const fixture=await api('/api/fixture'); await api('/api/reset',{method:'POST',body:JSON.stringify({fixture_json:fixture.fixture_json})}); await loadDatasets(); await loadRuns(); }
async function exportSnapshot(){ const s=await api('/api/snapshot'); const blob=new Blob([JSON.stringify(s,null,2)],{type:'application/json'}); const a=document.createElement('a');a.href=URL.createObjectURL(blob);a.download='concordia-runs.json';a.click();URL.revokeObjectURL(a.href); }
async function importSnapshot(file){ const text=await file.text(); JSON.parse(text); const result=await api('/api/snapshot',{method:'POST',body:text}); alert(`复核完成：${result.datasets} 数据集、${result.branches} 分支、${result.runs_verified} 运行`); await loadDatasets(); await loadRuns(); }
$('datasetSelect').onchange=selectDataset;  $('targetConvention').onchange=()=>runAnalysis(false); $('runBtn').onclick=()=>runAnalysis(false); $('branchBtn').onclick=saveBranch; $('compareBtn').onclick=compareBranches; $('resetBtn').onclick=reset; $('exportBtn').onclick=exportSnapshot; $('importFile').onchange=e=>importSnapshot(e.target.files[0]);
loadDatasets(); loadRuns();
