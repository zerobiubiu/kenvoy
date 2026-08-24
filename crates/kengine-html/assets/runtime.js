// kenvoy 客户端运行时：表格数据渲染 + 表单提交（走 `/__kenvoy/data` API）。
(function () {
  function escapeHtml(s) {
    return String(s == null ? "" : s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" }[c];
    });
  }
  // 表格数据渲染
  document.querySelectorAll("table[data-model]").forEach(function (table) {
    var model = encodeURIComponent(table.getAttribute("data-model"));
    var cols = Array.prototype.map.call(
      table.querySelectorAll("th[data-field]"),
      function (th) { return th.getAttribute("data-field"); }
    );
    fetch("/__kenvoy/data/" + model).then(function (r) { return r.json(); }).then(function (rows) {
      var tbody = table.querySelector("tbody");
      if (!rows.length) {
        tbody.innerHTML = '<tr><td class="kv-empty" colspan="' + cols.length + '">暂无数据</td></tr>';
        return;
      }
      tbody.innerHTML = rows.map(function (row) {
        return "<tr>" + cols.map(function (c) {
          return "<td>" + escapeHtml(row[c]) + "</td>";
        }).join("") + "</tr>";
      }).join("");
    });
  });
  // 表单提交
  document.querySelectorAll("form[data-model]").forEach(function (form) {
    var model = encodeURIComponent(form.getAttribute("data-model"));
    var submit = document.createElement("button");
    submit.type = "submit";
    submit.className = "btn btn-primary kv-submit";
    submit.textContent = "提交";
    form.appendChild(submit);
    form.addEventListener("submit", function (ev) {
      ev.preventDefault();
      var data = {};
      new FormData(form).forEach(function (v, k) { data[k] = v; });
      fetch("/__kenvoy/data/" + model, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(data),
      }).then(function (r) {
        if (!r.ok) {
          return r.json().then(function (e) { alert((e.errors || [e]).join("\n")); });
        }
        location.reload();
      });
    });
  });
})();
