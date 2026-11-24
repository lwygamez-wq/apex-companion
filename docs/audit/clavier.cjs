// Navigation au clavier, page par page : chaque contrôle visible est-il atteignable avec Tab, le focus est-il visible,
// et la navigation se fait-elle sans souris (Tab jusqu'à l'entrée du menu, Entrée) ?
const { chromium } = require('playwright');
const pages = ["Briefing","Stratégie","Setups","Au volant","Séances","Références","Progression","Réglages"];
(async () => {
  const b = await chromium.launch(); const p = await b.newPage({ viewport:{width:1280,height:800} });
  const errs=[]; p.on('pageerror', e=>errs.push(String(e).slice(0,160)));
  await p.goto('http://localhost:1430/preview.html' + (process.argv[2]||'')); await p.waitForTimeout(5000);
  const report=[];
  for (const name of pages) {
    // Aller à la page au clavier : focus sur le body, Tab jusqu'à l'entrée de menu, Entrée.
    await p.evaluate(()=>{ document.activeElement && document.activeElement.blur(); window.scrollTo(0,0); });
    let reached=false;
    for (let i=0;i<40;i++) { await p.keyboard.press('Tab'); const t=await p.evaluate(()=>document.activeElement?.classList.contains('nav-item') ? document.activeElement.textContent.trim() : null); if (t===name) { reached=true; break; } }
    if (!reached) { report.push({page:name, navReached:false}); continue; }
    await p.keyboard.press('Enter'); await p.waitForTimeout(3000);
    const active = await p.evaluate(()=>document.querySelector('.nav-item.active')?.textContent.trim());
    // Parcourir toute la page au Tab.
    const seen=new Set(); const noOutline=[]; let n=0, loops=0, lastKey='';
    for (let i=0;i<400;i++) {
      await p.keyboard.press('Tab');
      const f = await p.evaluate(()=>{ const e=document.activeElement; if(!e||e===document.body) return null; const r=e.getBoundingClientRect(); const cs=getComputedStyle(e);
        const vis = cs.outlineStyle!=='none' && parseFloat(cs.outlineWidth)>0 || (cs.boxShadow && cs.boxShadow!=='none');
        if(!e.dataset.kbd) e.dataset.kbd=String(Math.random());
        return { key:e.dataset.kbd, tag:e.tagName, txt:(e.textContent||e.getAttribute('aria-label')||e.getAttribute('title')||e.name||'').trim().slice(0,30), vis, w:r.width, h:r.height, nav:e.classList.contains('nav-item') }; });
      if (!f) continue;
      if (seen.has(f.key)) { loops++; if (loops>2) break; continue; }
      seen.add(f.key); n++;
      if (!f.vis) noOutline.push(`${f.tag}:${f.txt}`);
      if (f.w===0||f.h===0) noOutline.push(`INVISIBLE ${f.tag}:${f.txt}`);
    }
    // Contrôles visibles jamais atteints.
    const unreached = await p.evaluate(()=>[...document.querySelectorAll('button,a[href],select,input,textarea,[tabindex],summary')].filter(e=>{const r=e.getBoundingClientRect(); return r.width>0&&r.height>0&&!e.disabled&&e.tabIndex>=0&&!e.dataset.kbd && getComputedStyle(e).visibility!=='hidden';}).map(e=>`${e.tagName}:${(e.textContent||e.getAttribute('aria-label')||'').trim().slice(0,30)}`));
    report.push({ page:name, navReached:true, opened: active===name, focusables:n, noVisibleFocus:noOutline.slice(0,8), noVisibleFocusCount:noOutline.length, unreached:unreached.slice(0,8), unreachedCount:unreached.length });
    await p.evaluate(()=>document.querySelectorAll('[data-kbd]').forEach(e=>delete e.dataset.kbd));
  }
  console.log(JSON.stringify({report, errs}, null, 1)); await b.close();
})();
