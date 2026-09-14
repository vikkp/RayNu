(() => {
  const $ = (id) => document.getElementById(id);

  const fmtMonths = (n) => {
    const x = Number(n);
    if (Number.isNaN(x)) return String(n);
    return Number.isInteger(x) ? String(x) : x.toFixed(1).replace(/\.0$/, "");
  };

  const clampPct = (n) => Math.max(0, Math.min(100, Number(n) || 0));

  const apply = (data) => {
    const a = data.months_to_loi_a;
    const b = data.months_to_loi_b;
    const overall = data.overall_pct;
    const bars = data.bars || {};
    const pieces = data.pieces || {};

    if ($("loi-months")) $("loi-months").textContent = `${fmtMonths(a)} mo`;
    if ($("loi-delta")) {
      $("loi-delta").textContent = `Bar A · was ${fmtMonths(data.months_to_loi_a_prev)} · Bar B ${fmtMonths(b)} mo`;
    }
    if ($("loi-overall")) $("loi-overall").textContent = `${overall}%`;
    if ($("loi-eta")) $("loi-eta").textContent = data.loi_a_eta_month;
    if ($("loi-eta-b")) $("loi-eta-b").textContent = data.loi_b_eta_month;
    if ($("loi-confidence")) $("loi-confidence").textContent = data.confidence;
    if ($("loi-updated")) $("loi-updated").textContent = data.last_updated;
    if ($("loi-commit")) {
      $("loi-commit").textContent = data.last_commit_short || data.last_commit;
    }
    if ($("loi-path") && Array.isArray(data.loi_path)) {
      $("loi-path").textContent = data.loi_path.join(" → ");
    }

    const setBar = (fillId, labelId, pct) => {
      const p = clampPct(pct);
      if ($(fillId)) $(fillId).style.width = `${p}%`;
      if ($(labelId)) $(labelId).textContent = `${Math.round(p)}%`;
    };

    setBar("loi-bar-a", "loi-bar-a-label", bars.a);
    setBar("loi-bar-b", "loi-bar-b-label", bars.b);
    setBar("loi-bar-overall", "loi-bar-overall-label", overall);

    document.querySelectorAll("[data-piece]").forEach((el) => {
      const key = el.getAttribute("data-piece");
      if (key && pieces[key] != null) {
        el.textContent = `${pieces[key]}%`;
      }
    });
    document.querySelectorAll("[data-piece-fill]").forEach((el) => {
      const key = el.getAttribute("data-piece-fill");
      if (key && pieces[key] != null) {
        el.style.width = `${clampPct(pieces[key])}%`;
      }
    });
  };

  fetch("loi.json", { cache: "no-store" })
    .then((r) => {
      if (!r.ok) throw new Error(`loi.json ${r.status}`);
      return r.json();
    })
    .then(apply)
    .catch(() => {
      /* Static fallback numbers already in HTML. */
    });
})();
