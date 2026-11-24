// Audit de la carte : captures (modes, tailles, écrans, bords), chevauchement des étiquettes, temps de rendu.
const { chromium } = require('playwright');
const out = process.argv[2]; const tag = process.argv[3] || 'x';
const W = (process.argv[4] || '1280,1920,2560').split(',').map(Number);
(async () => {
  const b = await chromium.launch(); const report = [];
  for (const w of W) {
    const h = Math.round(w * 9 / 16);
    const p = await b.newPage({ viewport: { width: w, height: h } });
    const errs = []; p.on('pageerror', e => errs.push(String(e).slice(0, 160)));
    await p.goto('http://localhost:1430/preview.html'); await p.waitForTimeout(4000);
    await p.locator('.nav-item', { hasText: 'Séances' }).click(); await p.waitForTimeout(2500);
    await p.locator('.session-row').first().click(); await p.waitForTimeout(6000);
    const measure = async (label) => {
      const m = await p.evaluate(() => {
        const svg = document.querySelector('.trackmap-canvas'); if (!svg) return null;
        const boxes = [...svg.querySelectorAll('[data-label], g[style*="cursor: pointer"] > g > text, g[style*="cursor: pointer"] > circle')].map(e => { const r = e.getBoundingClientRect(); return { x0: r.left, y0: r.top, x1: r.right, y1: r.bottom, t: e.textContent || e.tagName }; }).filter(r => r.x1 > r.x0);
        let overlaps = 0; const pairs = [];
        // Une pastille et son propre texte se recouvrent par construction : on compare seulement les éléments de groupes différents.
        const groups = [...svg.querySelectorAll('[data-label-group], g[style*="cursor: pointer"]')];
        const gb = groups.map(g => [...g.querySelectorAll('circle,text,rect')].map(e => e.getBoundingClientRect()).filter(r => r.width > 0));
        for (let i = 0; i < gb.length; i++) for (let j = i + 1; j < gb.length; j++) {
          const hit = gb[i].some(a => gb[j].some(c => a.left < c.right - 1 && c.left < a.right - 1 && a.top < c.bottom - 1 && c.top < a.bottom - 1));
          if (hit) { overlaps++; pairs.push((groups[i].textContent || '').slice(0, 14) + ' × ' + (groups[j].textContent || '').slice(0, 14)); }
        }
        const tools = [...document.querySelectorAll('.trackmap-tools button, .trackmap-tools select')].filter(e => e.offsetParent).length;
        const minFont = Math.min(...[...svg.querySelectorAll('text')].map(t => t.getBoundingClientRect().height).filter(x => x > 0));
        return { labels: groups.length, overlaps, pairs: pairs.slice(0, 6), toolControls: tools, minTextPx: Math.round(minFont), paths: svg.querySelectorAll('path').length };
      });
      return { label, ...m };
    };
    const zoomPerf = async () => p.evaluate(async () => {
      const el = document.querySelector('.trackmap-canvas'); if (!el) return null;
      const r = el.getBoundingClientRect(); const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
      const times = []; let last = performance.now();
      for (let i = 0; i < 40; i++) {
        el.dispatchEvent(new WheelEvent('wheel', { deltaY: i < 20 ? -120 : 120, clientX: cx, clientY: cy, bubbles: true, cancelable: true }));
        await new Promise(res => requestAnimationFrame(() => requestAnimationFrame(res)));
        const now = performance.now(); times.push(now - last); last = now;
      }
      times.sort((a, b) => a - b);
      return { medianFrameMs: +(times[20] / 2).toFixed(1), p90FrameMs: +(times[36] / 2).toFixed(1) };
    });
    // Résumé : carte simple, mode par défaut.
    const sum = p.locator('.summary-map'); await sum.scrollIntoViewIfNeeded();
    report.push({ w, ...(await measure('resume-defaut')), perf: await zoomPerf() });
    await sum.screenshot({ path: `${out}/${tag}-${w}-resume.png` });
    // Options visibles + modes
    const opt = p.getByRole('button', { name: 'Options de la carte' }); if (await opt.count()) { await opt.click(); await p.waitForTimeout(400); }
    for (const [name, file] of [['Frein / gaz', 'pedales'], ['Vitesse', 'vitesse'], ['Gain / perte', 'delta']]) {
      const bt = p.locator('.summary-map .trackmap-tools button', { hasText: name }); if (await bt.count()) { await bt.first().click(); await p.waitForTimeout(500); }
      else { const sel = p.locator('.summary-map select'); if (await sel.count()) { await sel.first().selectOption({ label: { 'Frein / gaz': 'Pédales', 'Gain / perte': 'Écart de temps' }[name] ?? name }).catch(() => {}); await p.waitForTimeout(500); } }
      await sum.screenshot({ path: `${out}/${tag}-${w}-${file}.png` });
    }
    report.push({ w, ...(await measure('resume-outils')) });
    const edge = p.locator('.summary-map button', { hasText: /Bords/ });
    if (await edge.count()) { await edge.first().click(); await p.waitForTimeout(500); await sum.screenshot({ path: `${out}/${tag}-${w}-sans-bords.png` }); await edge.first().click(); }
    // Données : taille normale, puis 50 %, puis plein écran (studio)
    await p.getByRole('tab', { name: 'Données' }).click(); await p.waitForTimeout(2500);
    const side = p.locator('.analysis-side .trackmap').first();
    if (await side.count()) { await side.scrollIntoViewIfNeeded(); await side.screenshot({ path: `${out}/${tag}-${w}-donnees-normal.png` }); report.push({ w, ...(await measure('donnees-normal')) }); }
    const openMore = async () => { const m = p.locator('.trackmap-tools details.tm-more:not([open]) > summary'); if (await m.count()) { await m.first().click(); await p.waitForTimeout(200); } };
    await openMore();
    const half = p.locator('.trackmap-tools button', { hasText: '50 %' });
    if (await half.count()) { await half.first().click(); await p.waitForTimeout(1500); const big = p.locator('.replay-big .trackmap').first(); await big.scrollIntoViewIfNeeded(); await big.screenshot({ path: `${out}/${tag}-${w}-donnees-demi.png` }); report.push({ w, ...(await measure('donnees-demi')), perf: await zoomPerf() }); }
    await openMore();
    const fullb = p.locator('.trackmap-tools button', { hasText: /plein écran/ });
    if (await fullb.count()) { await fullb.first().click(); await p.waitForTimeout(2500); await p.screenshot({ path: `${out}/${tag}-${w}-plein.png` }); report.push({ w, ...(await measure('plein')), perf: await zoomPerf() }); }
    report.push({ w, errs });
    await p.close();
  }
  await b.close(); console.log(JSON.stringify(report, null, 1));
})();
