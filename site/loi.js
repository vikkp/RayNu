// LOI page: a derived view of site/sku.json (ADR-021 catalog + letters).
// No numbers of its own. No percent. No months.
(() => {
  const $ = (id) => document.getElementById(id);
  const STATUS_CLASS = { lived: "lived", "in flight": "flight", planned: "planned" };

  const esc = (s) =>
    String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const inline = (s) => esc(s).replace(/`([^`]+)`/g, "<code>$1</code>");
  const rowLink = (id) => `<a class="loi-letter__row" href="sku.html#${esc(id)}">${esc(id)}</a>`;

  const stack = (c) =>
    c.total
      ? `<div class="sku-stack loi-letter__stack" aria-hidden="true">${["lived", "in flight", "planned"]
          .map((k) => `<span class="sku-stack__seg sku-stack__seg--${STATUS_CLASS[k]}" style="width:${(100 * c[k]) / c.total}%"></span>`)
          .join("")}</div>`
      : "";

  const renderLetter = (L, data) => {
    const card = $(`letter-${L.id}`);
    if (!card) return;
    const c = L.counts;
    const byId = {};
    (data.areas || []).forEach((a) => a.items.forEach((it) => (byId[it.id] = it)));
    const open = L.rows.filter((r) => byId[r] && byId[r].status !== "lived");
    const extra = (L.extra || []).map((r) => `${rowLink(r)} <span class="loi-letter__extra-name">${inline(byId[r] ? byId[r].capability : "")}</span>`);
    card.classList.toggle("loi-letter--ready", L.ready);
    card.innerHTML = `
      <p class="loi-card__kicker">Letter ${esc(L.id)} · Bar ${esc(L.id)} · ${L.ready ? "ready" : "not yet"}</p>
      <h3>${esc(L.name)}</h3>
      <p class="loi-letter__meaning">${inline(L.meaning)}</p>
      <p class="loi-letter__rows">
        ${L.spec.includes("S1") ? `<strong>Shipment 1</strong> (${L.rows.length - (L.extra || []).length} rows, <a href="sku.html#ship-s1">the dedicated-box letter</a>)` : ""}
        ${extra.length ? `<br />plus ${extra.join(", ")}` : ""}
      </p>
      ${stack(c)}
      <p class="loi-letter__counts">
        <b class="sku-c sku-c--lived">${c.lived}</b> lived ·
        <b class="sku-c sku-c--flight">${c["in flight"]}</b> in flight ·
        <b class="sku-c sku-c--planned">${c.planned}</b> planned · ${c.total} rows
      </p>
      <p class="loi-letter__open">${
        L.ready
          ? "Every row is lived on the R640. The letter can be written."
          : `Not ready: ${open.length} row${open.length === 1 ? "" : "s"} not yet lived. The letter waits for the last of them; there is no partial letter.`
      }</p>`;
  };

  const renderNow = (data) => {
    const cur = (data.waves || []).find((w) => w.id === data.current_wave);
    if (!cur) {
      if ($("loi-now-title")) $("loi-now-title").textContent = "All numbered waves closed";
      return;
    }
    if ($("loi-now-title")) $("loi-now-title").textContent = `Wave ${cur.id} · ${cur.theme}`;
    if ($("loi-now-why")) $("loi-now-why").textContent = cur.why;
    const nu = data.next_up || [];
    if ($("loi-now-pick")) {
      $("loi-now-pick").innerHTML = nu.length
        ? `Picking up now: ${nu
            .map((n) => `<a href="sku.html#${esc(n.id)}">${esc(n.id)}</a>${n.role === "iron thread" ? " <span class=\"sku-now__role sku-now__role--iron\">iron thread</span>" : ""}`)
            .join(", ")}. Letter A is written when wave 9 closes; see <a href="sku.html#build-order">the waves</a>.`
        : "";
    }
  };

  fetch("sku.json", { cache: "no-store" })
    .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
    .then((data) => {
      (data.letters || []).forEach((L) => renderLetter(L, data));
      renderNow(data);
      if ($("loi-updated") && data.adr_date) {
        $("loi-updated").textContent = data.adr_date;
        $("loi-updated").setAttribute("datetime", data.adr_date);
      }
    })
    .catch(() => {
      const m = document.querySelector("#letter-A .loi-letter__meaning");
      if (m) m.textContent = "Could not load site/sku.json.";
    });
})();
