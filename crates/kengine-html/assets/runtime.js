// kenvoy 客户端运行时：登录态管理 + 表格数据渲染 + 表单提交 + 文件上传。
// 所有数据请求统一走 kvFetch（自动附带 Bearer token；401 时弹出登录框）。
(function () {
  var TOKEN_KEY = "kv_token";
  var USER_KEY = "kv_user";

  function getToken() { return localStorage.getItem(TOKEN_KEY); }
  function getUser() {
    try { return JSON.parse(localStorage.getItem(USER_KEY) || "null"); } catch (e) { return null; }
  }
  function setAuth(token, user) {
    if (token) localStorage.setItem(TOKEN_KEY, token); else localStorage.removeItem(TOKEN_KEY);
    if (user) localStorage.setItem(USER_KEY, JSON.stringify(user)); else localStorage.removeItem(USER_KEY);
  }

  function escapeHtml(s) {
    return String(s == null ? "" : s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" }[c];
    });
  }

  // ---- 登录模态框 ----
  function ensureLoginBox() {
    if (document.getElementById("kv-login-box")) return;
    var box = document.createElement("div");
    box.id = "kv-login-box";
    box.style.cssText = "display:none;position:fixed;inset:0;background:rgba(0,0,0,.45);z-index:9999;align-items:center;justify-content:center";
    box.innerHTML =
      '<div class="card" style="width:340px;max-width:90vw">' +
      '<div class="card-body">' +
      '<h3 class="card-title mb-3">登录 Kenvoy</h3>' +
      '<div id="kv-login-err" class="alert alert-danger py-1 px-2" style="display:none;font-size:.85rem"></div>' +
      '<div class="mb-3"><input id="kv-login-user" class="form-control" placeholder="用户名" autocomplete="username"></div>' +
      '<div class="mb-3"><input id="kv-login-pass" class="form-control" type="password" placeholder="密码" autocomplete="current-password"></div>' +
      '<button id="kv-login-btn" class="btn btn-primary w-100">登 录</button>' +
      '<button id="kv-logout-btn" class="btn btn-outline-secondary w-100 mt-2" style="display:none">退出登录</button>' +
      "</div></div>";
    document.body.appendChild(box);
  }

  var pendingAfterLogin = null; // 登录成功后待重试的请求

  function showLogin(onLogin) {
    ensureLoginBox();
    var box = document.getElementById("kv-login-box");
    var err = document.getElementById("kv-login-err");
    var user = getUser();
    document.getElementById("kv-logout-btn").style.display = user ? "block" : "none";
    if (user) {
      document.getElementById("kv-login-user").value = user.username || "";
    }
    err.style.display = "none";
    pendingAfterLogin = onLogin;
    box.style.display = "flex";
    document.getElementById("kv-login-user").focus();
  }

  function hideLogin() {
    var box = document.getElementById("kv-login-box");
    if (box) box.style.display = "none";
    pendingAfterLogin = null;
  }

  function doLogin() {
    var username = document.getElementById("kv-login-user").value.trim();
    var password = document.getElementById("kv-login-pass").value;
    var err = document.getElementById("kv-login-err");
    if (!username || !password) {
      err.textContent = "请输入用户名和密码";
      err.style.display = "block";
      return;
    }
    fetch("/__kenvoy/auth/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: username, password: password }),
    }).then(function (r) {
      return r.json().then(function (d) {
        if (!r.ok) throw new Error((d.errors || [d]).join("\n") || "登录失败（HTTP " + r.status + "）");
        return d;
      });
    }).then(function (d) {
      setAuth(d.token, d.user);
      hideLogin();
      var retry = pendingAfterLogin;
      pendingAfterLogin = null;
      if (retry) retry(); else location.reload();
    }).catch(function (e) {
      err.textContent = e.message;
      err.style.display = "block";
    });
  }

  function doLogout() {
    var token = getToken();
    hideLogin();
    setAuth(null, null);
    if (token) {
      fetch("/__kenvoy/auth/logout", { method: "POST", headers: { "Authorization": "Bearer " + token } })
        .catch(function () { /* 忽略登出失败 */ });
    }
    location.reload();
  }

  // ---- 统一请求：带 token；401 弹登录，登录成功重试 ----
  function kvFetch(url, options) {
    options = options || {};
    options.headers = options.headers || {};
    var token = getToken();
    if (token) options.headers["Authorization"] = "Bearer " + token;
    return fetch(url, options).then(function (r) {
      if (r.status === 401) {
        setAuth(null, null);
        return new Promise(function (resolve) {
          showLogin(function () { // 登录成功后重试原请求
            kvFetch(url, options).then(resolve);
          });
        });
      }
      return r;
    });
  }

  // ---- 表格数据渲染 ----
  document.querySelectorAll("table[data-model]").forEach(function (table) {
    var model = encodeURIComponent(table.getAttribute("data-model"));
    var cols = Array.prototype.map.call(
      table.querySelectorAll("th[data-field]"),
      function (th) { return { field: th.getAttribute("data-field"), type: th.getAttribute("data-type") || "text" }; }
    );
    var tbody = table.querySelector("tbody");
    var r = table.getAttribute("data-filter")
      ? "&filter=" + encodeURIComponent(table.getAttribute("data-filter"))
      : "";
    if (table.getAttribute("data-sort")) r += "&sort=" + encodeURIComponent(table.getAttribute("data-sort"));
    if (table.getAttribute("data-page-size")) r += "&pageSize=" + table.getAttribute("data-page-size");
    kvFetch("/__kenvoy/data/" + model + "?" + r.slice(1))
      .then(function (resp) {
        if (!resp.ok) {
          // 静态快照降级：读相对路径 __kenvoy/data/{model}.json
          return fetch("__kenvoy/data/" + model + ".json").then(function (r2) { return r2.json(); });
        }
        return resp.json();
      })
      .then(function (rows) {
        if (!rows.length) {
          tbody.innerHTML = '<tr><td class="kv-empty" colspan="' + cols.length + '">暂无数据</td></tr>';
          return;
        }
        tbody.innerHTML = rows.map(function (row) {
          return "<tr>" + cols.map(function (c) {
            var v = row[c.field];
            if (c.type === "file" && v) {
              return "<td><a href=\"/__kenvoy/file/" + encodeURIComponent(v) + "\" target=\"_blank\">下载</a></td>";
            }
            if (v && typeof v === "object") v = JSON.stringify(v);
            return "<td>" + escapeHtml(v) + "</td>";
          }).join("") + "</tr>";
        }).join("");
      })
      .catch(function () { tbody.innerHTML = '<tr><td class="kv-empty" colspan="' + cols.length + '">数据加载失败</td></tr>'; });
  });

  // ---- 表单提交 ----
  document.querySelectorAll("form[data-model]").forEach(function (form) {
    var model = encodeURIComponent(form.getAttribute("data-model"));
    var submit = document.createElement("button");
    submit.type = "submit";
    submit.className = "btn btn-primary kv-submit";
    submit.textContent = "提交";
    form.appendChild(submit);
    form.addEventListener("submit", function (ev) {
      ev.preventDefault();
      // 有选中文件的 file 控件先上传到 /__kenvoy/file，payload 中写入文件 id
      var uploads = Array.prototype.map.call(
        form.querySelectorAll('input[type="file"]'),
        function (input) {
          if (!input.files || !input.files.length) return Promise.resolve(null);
          var fd = new FormData();
          fd.append("file", input.files[0]);
          return kvFetch("/__kenvoy/file", { method: "POST", body: fd })
            .then(function (r) {
              if (!r.ok) return r.json().then(function (e) { throw new Error((e.errors || [e]).join("\n")); });
              return r.json();
            })
            .then(function (j) { return { name: input.name, id: j.id }; })
            .catch(function (e) { return { error: e.message }; });
        }
      );
      Promise.all(uploads).then(function (results) {
        var failed = results.filter(function (r) { return r && r.error; });
        if (failed.length) {
          alert("文件上传失败：" + failed[0].error);
          return;
        }
        var data = {};
        new FormData(form).forEach(function (v, k) { if (!(v instanceof File)) data[k] = v; });
        results.forEach(function (r) { if (r) data[r.name] = r.id; });
        kvFetch("/__kenvoy/data/" + model, {
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
  });

  // ---- 统计图表（柱状图） ----
  document.querySelectorAll(".kv-chart").forEach(function (el) {
    var model = encodeURIComponent(el.getAttribute("data-model"));
    var field = el.getAttribute("data-field");
    var stat = el.getAttribute("data-stat") || "count";
    kvFetch("/__kenvoy/stats/" + model + "?group=" + encodeURIComponent(field) + "&stat=" + stat)
      .then(function (r) { return r.json(); })
      .then(function (rows) {
        if (!rows.length) { el.innerHTML = '<div class="kv-empty">暂无统计</div>'; return; }
        var max = Math.max.apply(null, rows.map(function (r) { return r.value; }));
        var w = 600, h = 240, bw = Math.max(20, (w - 80) / rows.length - 8);
        var bars = rows.map(function (r, i) {
          var bh = max ? (r.value / max) * (h - 60) : 0;
          var x = 44 + i * (bw + 8);
          return '<rect x="' + x + '" y="' + (h - 44 - bh) + '" width="' + bw + '" height="' + bh + '" fill="#4b6bfb" rx="3">' +
                 '<title>' + escapeHtml(r.group) + ': ' + r.value + '</title></rect>' +
                 '<text x="' + (x + bw / 2) + '" y="' + (h - 26) + '" text-anchor="middle" font-size="11">' +
                 escapeHtml(String(r.group)) + '</text>' +
                 '<text x="' + (x + bw / 2) + '" y="' + Math.max(14, h - 48 - bh) + '" text-anchor="middle" font-size="11" fill="#6c757d">' +
                 r.value + '</text>';
        }).join("");
        el.innerHTML = '<svg viewBox="0 0 ' + w + ' ' + h + '" style="width:100%;height:auto">' + bars + '</svg>';
      })
      .catch(function () { el.innerHTML = '<div class="kv-empty">统计加载失败</div>'; });
  });

  // ---- 登录 / 登出 / Enter 事件 ----
  ensureLoginBox();
  document.addEventListener("click", function (ev) {
    if (ev.target && ev.target.id === "kv-login-btn") doLogin();
    if (ev.target && ev.target.id === "kv-logout-btn") doLogout();
  });
  document.addEventListener("keydown", function (ev) {
    if (ev.key === "Enter" && document.getElementById("kv-login-box") &&
        document.getElementById("kv-login-box").style.display === "flex") {
      doLogin();
    }
  });
})();