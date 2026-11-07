(() => {
  const $ = (id) => document.getElementById(id);

  const fmtNum = (n) => {
    const x = Number(n);
    if (!Number.isFinite(x)) return String(n);
    return String(x);
  };

  const clampPct = (n) => Math.max(0, Math.min(100, Number(n) || 0));

  const fmtPct = (n) => `${fmtNum(clampPct(n))}%`;

  const lookup = (spec, data, bars, pieces) => {
    if (spec === "overall") return data.overall_pct;
    if (spec === "months.a") return data.months_to_loi_a;
    if (spec === "months.b") return data.months_to_loi_b;
    if (spec.startsWith("bars.")) return bars[spec.slice(5)];
    if (spec.startsWith("pieces.")) return pieces[spec.slice(7)];
    return undefined;
  };

  const apply = (data) => {
    const a = data.months_to_loi_a;
    const b = data.months_to_loi_b;
    const overall = data.overall_pct;
    const bars = data.bars || {};
    const pieces = data.pieces || {};

    if ($("loi-months")) $("loi-months").textContent = `${fmtNum(a)} mo`;
    if ($("loi-delta")) {
      $("loi-delta").textContent = `Bar A · was ${fmtNum(data.months_to_loi_a_prev)} · Bar B ${fmtNum(b)} mo`;
    }
    if ($("loi-overall")) $("loi-overall").textContent = fmtPct(overall);
    if ($("loi-eta")) $("loi-eta").textContent = data.loi_a_eta_month;
    if ($("loi-eta-b")) $("loi-eta-b").textContent = data.loi_b_eta_month;
    if ($("loi-confidence")) $("loi-confidence").textContent = data.confidence;
    if ($("loi-updated")) $("loi-updated").textContent = data.last_updated;
    if ($("loi-commit")) {
      $("loi-commit").textContent = data.last_commit_short || data.last_commit;
    }
    if ($("loi-path")) {
      const bit = (label, spec) => `${label} ${fmtPct(lookup(spec, data, bars, pieces))}`;
      $("loi-path").textContent = [
        bit("Everest", "pieces.everest"),
        bit("Persist", "pieces.persist"),
        bit("SKU", "pieces.sku"),
        bit("TLS", "pieces.tls"),
        bit("Auth", "pieces.auth"),
        bit("Console", "pieces.console"),
        bit("PERC", "pieces.perc"),
        bit("Unmodified", "pieces.unmodified"),
        bit("Bar A", "bars.a"),
        bit("Bar B", "bars.b"),
        bit("Overall", "overall"),
      ].join(" → ");
    }

    const setBar = (fillId, labelId, pct) => {
      const p = clampPct(pct);
      if ($(fillId)) $(fillId).style.width = `${fmtNum(p)}%`;
      if ($(labelId)) $(labelId).textContent = fmtPct(pct);
    };

    setBar("loi-bar-a", "loi-bar-a-label", bars.a);
    setBar("loi-bar-b", "loi-bar-b-label", bars.b);
    setBar("loi-bar-overall", "loi-bar-overall-label", overall);

    document.querySelectorAll("[data-bar-kicker]").forEach((el) => {
      const key = el.getAttribute("data-bar-kicker");
      if (key && bars[key] != null) {
        const name = key === "b" ? "Bar B" : "Bar A";
        el.textContent = `${name} · ${fmtPct(bars[key])}`;
      }
    });

    document.querySelectorAll("[data-loi-pct]").forEach((el) => {
      const spec = el.getAttribute("data-loi-pct");
      const v = lookup(spec, data, bars, pieces);
      if (v == null) return;
      el.textContent = spec.startsWith("months.") ? fmtNum(v) : fmtPct(v);
    });

    document.querySelectorAll("[data-piece]").forEach((el) => {
      const key = el.getAttribute("data-piece");
      if (key && pieces[key] != null) {
        el.textContent = fmtPct(pieces[key]);
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
