(() => {
  const $ = (id) => document.getElementById(id);
  const STATUS_CLASS = { lived: "lived", "in flight": "flight", planned: "planned" };

  const state = { view: "Final", status: "all", q: "" };
  let data = null;

  const esc = (s) =>
    String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

  // Backticked spans in notes become <code>.
  const inline = (s) => esc(s).replace(/`([^`]+)`/g, "<code>$1</code>");

  const pill = (status) =>
    `<span class="sku-pill sku-pill--${STATUS_CLASS[status] || "planned"}">${esc(status)}</span>`;

  const renderCounts = (el, c) => {
    if (!el) return;
    el.innerHTML = [
      ["lived", c.lived],
      ["in flight", c["in flight"]],
      ["planned", c.planned],
      ["total", c.total],
    ]
      .map(
        ([k, v]) =>
          `<div class="sku-ship__count sku-ship__count--${STATUS_CLASS[k] || "total"}"><dt>${esc(k)}</dt><dd>${v}</dd></div>`
      )
      .join("");
  };

  const renderStack = (el, c) => {
    if (!el || !c.total) return;
    const seg = (k) =>
      `<span class="sku-stack__seg sku-stack__seg--${STATUS_CLASS[k]}" style="width:${(100 * c[k]) / c.total}%" title="${esc(k)} ${c[k]}"></span>`;
    el.innerHTML = seg("lived") + seg("in flight") + seg("planned");
  };

  const matches = (it) => {
    if (state.view === "S1" && it.ships !== "S1") return false;
    if (state.status !== "all" && it.status !== state.status) return false;
    if (state.q) {
      const hay = `${it.id} ${it.capability} ${it.ref} ${it.note} ${it.status}`.toLowerCase();
      if (!hay.includes(state.q)) return false;
    }
    return true;
  };

  const renderCatalog = () => {
    const root = $("sku-catalog");
    if (!root || !data) return;
    let shown = 0;
    let total = 0;
    const html = data.areas
      .map((a) => {
        const items = a.items.filter((it) => state.view !== "S1" || it.ships === "S1");
        total += items.length;
        const visible = items.filter(matches);
        shown += visible.length;
        if (visible.length === 0) return "";
        const counts = { lived: 0, "in flight": 0, planned: 0 };
        items.forEach((it) => (counts[it.status] += 1));
        const tags = (a.pillars || []).map((p) => `<span class="sku-pillar">[${esc(p)}]</span>`).join("");
        const rows = visible
          .map(
            (it) => `
          <li class="sku-row sku-row--${STATUS_CLASS[it.status]}" id="${esc(it.id)}">
            <span class="sku-row__id">${esc(it.id)}</span>
            <div class="sku-row__body">
              <p class="sku-row__cap">${inline(it.capability)}</p>
              <p class="sku-row__meta">
                ${pill(it.status)}
                <span class="sku-ships sku-ships--${esc(it.ships).toLowerCase()}">${it.ships === "S1" ? "Shipment 1" : "Final"}</span>
                ${it.ref ? `<span class="sku-ref">${inline(it.ref)}</span>` : ""}
              </p>
              ${it.note ? `<p class="sku-row__note">${inline(it.note)}</p>` : ""}
            </div>
          </li>`
          )
          .join("");
        return `
        <section class="sku-area" id="area-${esc(a.id)}">
          <header class="sku-area__head">
            <span class="sku-area__letter">${esc(a.id)}</span>
            <h3 class="sku-area__name">${esc(a.name)} ${tags}</h3>
            <span class="sku-area__counts">
              <b class="sku-c sku-c--lived">${counts.lived}</b> lived ·
              <b class="sku-c sku-c--flight">${counts["in flight"]}</b> in flight ·
              <b class="sku-c sku-c--planned">${counts.planned}</b> planned
            </span>
          </header>
          <ol class="sku-rows">${rows}</ol>
        </section>`;
      })
      .join("");
    root.innerHTML = html || `<p class="sku-empty">No rows match. Clear the search or widen the status filter.</p>`;
    const viewName = state.view === "S1" ? "Shipment 1" : "the final SKU";
    const statusName = state.status === "all" ? "" : ` marked ${state.status}`;
    const q = state.q ? ` containing “${state.q}”` : "";
    if ($("sku-showing")) {
      $("sku-showing").textContent = `Showing ${shown} of ${total} rows in ${viewName}${statusName}${q}.`;
    }
  };

  const renderAreasNav = () => {
    const nav = $("sku-areas-nav");
    if (!nav || !data) return;
    nav.innerHTML = data.areas
      .map((a) => `<a href="#area-${esc(a.id)}"><b>${esc(a.id)}</b> ${esc(a.name)}</a>`)
      .join("");
  };

  const syncChips = () => {
    document.querySelectorAll(".sku-chip[data-view]").forEach((b) => {
      b.classList.toggle("is-on", b.dataset.view === state.view);
      b.setAttribute("aria-pressed", String(b.dataset.view === state.view));
    });
    document.querySelectorAll(".sku-chip[data-status]").forEach((b) => {
      b.classList.toggle("is-on", b.dataset.status === state.status);
      b.setAttribute("aria-pressed", String(b.dataset.status === state.status));
    });
    document.body.classList.toggle("page-sku--s1", state.view === "S1");
  };

  const wire = () => {
    document.querySelectorAll("[data-view]").forEach((b) => {
      b.addEventListener("click", (ev) => {
        if (b.tagName === "A") ev.preventDefault();
        state.view = b.dataset.view;
        syncChips();
        renderCatalog();
        if (b.tagName === "A") {
          const target = $("catalog");
          if (target) target.scrollIntoView({ behavior: "smooth", block: "start" });
        }
      });
    });
    document.querySelectorAll(".sku-chip[data-status]").forEach((b) => {
      b.addEventListener("click", () => {
        state.status = b.dataset.status;
        syncChips();
        renderCatalog();
      });
    });
    const search = $("sku-search");
    if (search) {
      search.addEventListener("input", () => {
        state.q = search.value.trim().toLowerCase();
        renderCatalog();
      });
    }
    const hashView = new URLSearchParams(location.search).get("view");
    if (hashView === "S1" || hashView === "Final") state.view = hashView;
  };

  const apply = (d) => {
    data = d;
    if ($("sku-date")) $("sku-date").textContent = d.adr_date || "—";
    if ($("sku-total")) $("sku-total").textContent = d.counts.Final.total;
    if ($("sku-s1-lede")) $("sku-s1-lede").textContent = d.shipments.S1.lede;
    if ($("sku-final-lede")) $("sku-final-lede").textContent = d.shipments.Final.lede;
    renderCounts($("sku-s1-counts"), d.counts.S1);
    renderCounts($("sku-final-counts"), d.counts.Final);
    renderStack($("sku-s1-stack"), d.counts.S1);
    renderStack($("sku-final-stack"), d.counts.Final);
    renderAreasNav();
    syncChips();
    renderCatalog();
    if (location.hash && /^#[A-K]\d+$/.test(location.hash)) {
      const row = document.querySelector(location.hash);
      if (row) {
        row.scrollIntoView({ block: "center" });
        row.classList.add("is-target");
      }
    }
  };

  wire();
  fetch("sku.json", { cache: "no-store" })
    .then((r) => {
      if (!r.ok) throw new Error(`sku.json ${r.status}`);
      return r.json();
    })
    .then(apply)
    .catch((err) => {
      const root = $("sku-catalog");
      if (root) {
        root.innerHTML = `<p class="sku-empty">Could not load <code>sku.json</code> (${esc(err.message)}). The list is in <a href="https://github.com/vikkp/RayNu/blob/main/docs/adr/ADR-021.md#catalog">ADR-021</a>.</p>`;
      }
    });
})();
