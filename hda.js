(() => {
  const $ = (id) => document.getElementById(id);

  const fmtMonths = (n) => {
    const x = Number(n);
    if (Number.isNaN(x)) return String(n);
    return Number.isInteger(x) ? String(x) : x.toFixed(1).replace(/\.0$/, "");
  };

  const monthsBarWidth = (months, baseline) => {
    const m = Number(months);
    const b = Number(baseline) || 4.5;
    if (!(m >= 0) || !(b > 0)) return 40;
    // Full bar ≈ baseline months; shrinks as we get closer.
    return Math.max(8, Math.min(100, Math.round((m / b) * 100)));
  };

  const apply = (data) => {
    const months = data.months_to_everest;
    const prev = data.months_to_everest_prev;
    const overall = data.overall_pct;
    const baseline = data.baseline_months || 4.5;
    const summits = data.summits || {};

    if ($("hda-months")) $("hda-months").textContent = fmtMonths(months);
    if ($("hda-delta")) {
      const delta = Number(months) - Number(prev);
      let label = `was ${fmtMonths(prev)}`;
      if (!Number.isNaN(delta) && delta !== 0) {
        const sign = delta < 0 ? "−" : "+";
        label = `${sign}${fmtMonths(Math.abs(delta))} mo · was ${fmtMonths(prev)}`;
      }
      $("hda-delta").textContent = label;
    }
    if ($("hda-overall")) $("hda-overall").textContent = `${overall}%`;
    if ($("hda-eta")) $("hda-eta").textContent = data.everest_eta_month;
    if ($("hda-confidence")) $("hda-confidence").textContent = data.confidence;
    if ($("hda-updated")) $("hda-updated").textContent = data.last_updated;
    if ($("hda-commit")) {
      $("hda-commit").textContent = data.last_commit_short || data.last_commit;
    }
    if ($("hda-path") && Array.isArray(data.mount_everest_path)) {
      $("hda-path").textContent = data.mount_everest_path.join(" → ");
    }
    if ($("hda-bar-months-label")) {
      $("hda-bar-months-label").textContent = `${fmtMonths(months)} mo`;
    }
    if ($("hda-bar-overall-label")) {
      $("hda-bar-overall-label").textContent = `${overall}%`;
    }
    if ($("hda-bar-months")) {
      $("hda-bar-months").style.width = `${monthsBarWidth(months, baseline)}%`;
    }
    if ($("hda-bar-overall")) {
      $("hda-bar-overall").style.width = `${Math.max(0, Math.min(100, overall))}%`;
    }
    if ($("hda-prod") && summits.prod != null) {
      $("hda-prod").textContent = `${summits.prod}%`;
    }
    if ($("hda-core") && summits.core != null) {
      $("hda-core").textContent = String(summits.core);
    }
    document.querySelectorAll("[data-summit]").forEach((el) => {
      const key = el.getAttribute("data-summit");
      if (key && summits[key] != null) {
        el.textContent = `${summits[key]}%`;
      }
    });
  };

  fetch("hda.json", { cache: "no-store" })
    .then((r) => {
      if (!r.ok) throw new Error(`hda.json ${r.status}`);
      return r.json();
    })
    .then(apply)
    .catch(() => {
      /* Static fallback numbers already in HTML. */
    });
})();
