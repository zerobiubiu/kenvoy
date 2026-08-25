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
      '<h3 class="card-title mb-3" id="kv-login-title">登录 Kenvoy</h3>' +
      '<div id="kv-login-err" class="alert alert-danger py-1 px-2" style="display:none;font-size:.85rem"></div>' +
      '<div id="kv-login-fields"><div class="mb-3"><input id="kv-login-user" class="form-control" placeholder="用户名" autocomplete="username"></div>' +
      '<div class="mb-3"><input id="kv-login-pass" class="form-control" type="password" placeholder="密码" autocomplete="current-password"></div></div>' +
      '<button id="kv-login-btn" class="btn btn-primary w-100">登 录</button>' +
      '<div class="d-flex justify-content-between mt-2">' +
      '<button id="kv-login-switch" class="btn btn-link btn-sm p-0" type="button">注册账号</button>' +
      '<button id="kv-login-forgot" class="btn btn-link btn-sm p-0" type="button">忘记密码</button>' +
      '</div>' +
      '<button id="kv-logout-btn" class="btn btn-outline-secondary w-100 mt-3" style="display:none">退出登录</button>' +
      "</div></div>";
    document.body.appendChild(box);
    // hr 模式：隐藏公开注册入口（只能由人事应用创建账户）。
    var mode = (document.body && document.body.getAttribute("data-auth-mode")) || "open";
    if (mode === "hr") {
      var sw = document.getElementById("kv-login-switch");
      if (sw) sw.parentNode.removeChild(sw);
    }
  }

  // ---- 顶部导航：登录态徽章（用户名 / 退出）与未登录登录按钮 ----
  function ensureUserBadge() {
    if (document.getElementById("kv-user-badge")) return;
    var nav = document.querySelector(".navbar-nav");
    if (!nav) return;
    var li = document.createElement("li");
    li.id = "kv-user-badge";
    li.className = "nav-item ms-auto d-flex align-items-center gap-2";
    var user = getUser();
    if (user) {
      var span = document.createElement("span");
      span.className = "nav-link text-muted small py-1";
      span.textContent = (user.display_name || user.username || "用户");
      var out = document.createElement("button");
      out.id = "kv-user-logout";
      out.className = "btn btn-sm btn-outline-secondary";
      out.type = "button";
      out.textContent = "退出";
      li.appendChild(span);
      li.appendChild(out);
    } else {
      var login = document.createElement("button");
      login.id = "kv-user-login-top";
      login.className = "btn btn-sm btn-primary";
      login.type = "button";
      login.textContent = "登录 / 注册";
      li.appendChild(login);
    }
    nav.appendChild(li);
  }

  // ---- 未登录引导条（页面内有 `[data-kv-guest-hint]` 元素时显示） ----
  function initGuestHint() {
    var hints = document.querySelectorAll("[data-kv-guest-hint]");
    if (!hints.length) return;
    if (getUser()) {
      hints.forEach(function (h) { h.style.display = "none"; });
      return;
    }
    hints.forEach(function (h) { h.style.display = ""; });
    var btn = document.querySelector("[data-kv-guest-login]");
    if (btn) {
      btn.addEventListener("click", function () { showLogin(); });
    }
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
    setLoginMode(getLoginMode());
    box.style.display = "flex";
    var focusEl = document.getElementById(getLoginMode() === "register" ? "kv-login-user" : "kv-login-user");
    if (focusEl) focusEl.focus();
  }

  // 登录界面模式：'login' / 'register' / 'forgot'
  function getLoginMode() {
    return document.getElementById("kv-login-box").getAttribute("data-mode") || "login";
  }
  function setLoginMode(mode) {
    var box = document.getElementById("kv-login-box");
    box.setAttribute("data-mode", mode);
    var title = document.getElementById("kv-login-title");
    var btn = document.getElementById("kv-login-btn");
    var fields = document.getElementById("kv-login-fields");
    var pass = document.getElementById("kv-login-pass");
    if (mode === "register") {
      title.textContent = "注册 Kenvoy 账号";
      btn.textContent = "注 册";
      // 注册多一个“显示名”字段
      if (!document.getElementById("kv-login-display")) {
        var wrap = document.createElement("div");
        wrap.className = "mb-3";
        wrap.innerHTML = '<input id="kv-login-display" class="form-control" placeholder="显示名">';
        fields.insertBefore(wrap, pass.parentNode);
      }
    } else if (mode === "forgot") {
      title.textContent = "重置密码";
      btn.textContent = "发送重置链接";
      var disp = document.getElementById("kv-login-display");
      if (disp) disp.parentNode.removeChild(disp);
    } else {
      title.textContent = "登录 Kenvoy";
      btn.textContent = "登 录";
      var disp2 = document.getElementById("kv-login-display");
      if (disp2) disp2.parentNode.removeChild(disp2);
    }
    err.style.display = "none";
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

  // 注册：调 `POST /__kenvoy/auth/register`；需数据库允许（open/review 模式）。
  // hr 模式（关闭公开注册）将收到 403。
  function doRegister() {
    var username = document.getElementById("kv-login-user").value.trim();
    var display = (document.getElementById("kv-login-display") || { value: "" }).value.trim();
    var password = document.getElementById("kv-login-pass").value;
    var err = document.getElementById("kv-login-err");
    if (!username || !password || !display) {
      err.textContent = "请填写用户名、显示名和密码";
      err.style.display = "block";
      return;
    }
    fetch("/__kenvoy/auth/register", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: username, password: password, display_name: display }),
    }).then(function (r) {
      return r.json().then(function (d) {
        if (!r.ok) throw new Error((d.errors || [d]).join("\n") || "注册失败（HTTP " + r.status + "）");
        return d;
      });
    }).then(function () {
      // 注册成功后自动尝试登录
      setLoginMode("login");
      document.getElementById("kv-login-pass").value = password;
      doLogin();
    }).catch(function (e) {
      err.textContent = e.message;
      err.style.display = "block";
    });
  }

  // 忘记密码：调 `POST /__kenvoy/auth/forgot` 生成重置 token。
  // 生产环境该 token 由邮件发送给用户；dev 模式服务端返回 devToken 供调试。
  function doforgot() {
    var username = document.getElementById("kv-login-user").value.trim();
    var err = document.getElementById("kv-login-err");
    if (!username) {
      err.textContent = "请输入用户名";
      err.style.display = "block";
      return;
    }
    fetch("/__kenvoy/auth/forgot", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: username }),
    }).then(function (r) { return r.json(); })
      .then(function (d) {
        err.className = "alert alert-info py-1 px-2";
        if (d.devToken) {
          err.innerHTML = "重置 token 已生成（服务器控制台含明文 token；实际部署请接入邮件发送）" +
            "<br><code style=\"font-size:.75rem\">" + d.devToken + "</code>";
        } else {
          err.textContent = "如果账号存在，重置链接已发送（请检查邮箱）";
        }
        err.style.display = "block";
      })
      .catch(function (e) {
        err.className = "alert alert-danger py-1 px-2";
        err.textContent = "请求失败：" + e.message;
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

  // ---- 加载 User / Department 下拉选项（仅登录用户可调） ----
  function loadOptions(select) {
    var source = select.getAttribute("data-source");
    if (!source) return;
    var url = "/__kenvoy/auth/options/" + source;
    kvFetch(url).then(function (r) {
      if (!r.ok) { select.innerHTML = '<option value="">(加载失败)</option>'; return; }
      return r.json();
    }).then(function (rows) {
      if (!rows) return;
      var current = select.value;
      var label = select.getAttribute("data-type") === "user" ? "displayName" : "name";
      var html = '<option value="">请选择…</option>';
      for (var i = 0; i < rows.length; i++) {
        var r = rows[i];
        var sel = ("" + r.id) === current ? " selected" : "";
        html += '<option value="' + r.id + '"' + sel + '>' +
                escapeHtml(r[label] || r.username || r.id) + '</option>';
      }
      select.innerHTML = html;
    }).catch(function () { select.innerHTML = '<option value="">(加载失败)</option>'; });
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
          var recordId = row.id != null ? row.id : "";
          var flowName = table.getAttribute("data-flow") || "";
          return "<tr data-record-id=\"" + encodeURIComponent(recordId) + "\">" + cols.map(function (c) {
            var v = row[c.field];
            if (c.field === "_actions") {
              // 操作列：无流程名则不渲染；有则按协议 data-kv-flow 渲染三个按钮
              if (!flowName) return "<td></td>";
              var r = encodeURIComponent(recordId);
              var f = encodeURIComponent(flowName);
              return "<td class=\"text-nowrap\">" +
                "<button data-kv-flow=\"withdraw\" data-flow=\"" + f + "\" data-record-id=\"" + r + "\" data-confirm=\"确认撤回该申请？\" class=\"btn btn-sm btn-outline-warning me-1\">撤回</button>" +
                "<button data-kv-flow=\"approve\" data-flow=\"" + f + "\" data-record-id=\"" + r + "\" class=\"btn btn-sm btn-outline-success me-1\">通过</button>" +
                "<button data-kv-flow=\"reject\" data-flow=\"" + f + "\" data-record-id=\"" + r + "\" class=\"btn btn-sm btn-outline-danger\">驳回</button></td>";
            }
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

  // ---- 加载 User/Department 下拉选项（页面加载后） ----
  document.querySelectorAll("select[data-source]").forEach(function (sel) { loadOptions(sel); });

  // ---- 审批流按钮 ----
  // DSL 渲染审批操作按钮时使用以下属性：
  //   <button data-kv-flow="start|withdraw|approve|reject|add-sign"
  //           data-flow="流程名" data-record-id="1"
  //           data-node="加签节点名" data-confirm="二次确认文案">
  // runtime.js 负责调用对应端点，失败提示，成功后刷新页面。
  // 表单提交到点（e.g. 发起审批）也可由表单字段反填 recordId 后续接上。
  var FLOW_ENDPOINTS = {
    start: "start",
    withdraw: "withdraw",
    "add-sign": "add-sign",
  };
  document.addEventListener("click", function (ev) {
    var btn = ev.target.closest && ev.target.closest("[data-kv-flow]");
    if (!btn) return;
    ev.preventDefault();
    var action = btn.getAttribute("data-kv-flow");
    var flow = btn.getAttribute("data-flow");
    var recordId = btn.getAttribute("data-record-id");
    var node = btn.getAttribute("data-node");
    var confirmMsg = btn.getAttribute("data-confirm");
    if (!flow || !recordId) { alert("审批按钮缺 data-flow 或 data-record-id"); return; }
    if (confirmMsg && !window.confirm(confirmMsg)) return;
    var url, body = null;
    if (action === "approve" || action === "reject") {
      url = "/__kenvoy/flow/" + encodeURIComponent(flow) +
            "/records/" + encodeURIComponent(recordId) + "/step";
      body = JSON.stringify({ action: action });
    } else if (action === "add-sign") {
      url = "/__kenvoy/flow/" + encodeURIComponent(flow) +
            "/records/" + encodeURIComponent(recordId) + "/add-sign";
      body = JSON.stringify({ node: node || "" });
    } else if (FLOW_ENDPOINTS[action]) {
      url = "/__kenvoy/flow/" + encodeURIComponent(flow) +
            "/records/" + encodeURIComponent(recordId) + "/" + FLOW_ENDPOINTS[action];
    } else {
      alert("未知审批动作：" + action);
      return;
    }
    var opts = { method: "POST" };
    if (body) {
      opts.headers = { "Content-Type": "application/json" };
      opts.body = body;
    }
    kvFetch(url, opts).then(function (r) {
      return r.json().then(function (j) {
        if (!r.ok) {
          alert("审批操作失败：" + (j.message || j.errors || JSON.stringify(j)));
          return;
        }
        location.reload();
      });
    }).catch(function (e) { alert("审批操作异常：" + e); });
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

  // ---- 事件绑定（登录 / 登出 / Enter / 模式切换） ----
  ensureLoginBox();
  ensureUserBadge();
  initGuestHint();
  document.addEventListener("click", function (ev) {
    if (ev.target && ev.target.id === "kv-login-btn") {
      var mode = getLoginMode();
      if (mode === "register") doRegister();
      else if (mode === "forgot") doforgot();
      else doLogin();
    }
    if (ev.target && ev.target.id === "kv-logout-btn") doLogout();
    if (ev.target && ev.target.id === "kv-user-logout") doLogout();
    if (ev.target && ev.target.id === "kv-user-login-top") showLogin();
    if (ev.target && ev.target.id === "kv-login-switch") {
      setLoginMode(getLoginMode() === "register" ? "login" : "register");
    }
    if (ev.target && ev.target.id === "kv-login-forgot") {
      setLoginMode("forgot");
    }
  });
  document.addEventListener("keydown", function (ev) {
    if (ev.key === "Enter" && document.getElementById("kv-login-box") &&
        document.getElementById("kv-login-box").style.display === "flex") {
      doLogin();
    }
  });
})();