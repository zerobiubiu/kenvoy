#!/bin/bash
# examples/leave-app/seed.sh —— 引导脚本：建角色 / 部门 / 演示账户
#
# 用法（在 examples/leave-app 目录）：
#   bash seed.sh                          # 完整 bootstrap（缺则创、存在则跳）
#   bash seed.sh --reset                  # 删除 demo 数据后重建
#   bash seed.sh --reset-users            # 仅删除 demo 用户
#
# 演示账户（密码均为 `demo123`，强度策略为 `none` 允许）：
#   admin / demo123  角色 管理员          — 可访问所有管理端点
#   hr     / demo123  角色 HR             — 通讯录/部门管理
#   ceo    / demo123  角色 人事总监        — >3 天请假审批
#   alice  / demo123  角色 部门主管        — 请假审批
#   bob    / demo123  角色 普通员工        — 提交请假
#   carol  / demo123  角色 —             — 仅通讯录（无账号人员）
#
# 账号等级 = 所分配角色的等级（`kv role add --level`）。

set -e
cd "$(dirname "$0")"

# --- 选项 ---
RESET=0
RESET_USERS=0
for arg in "$@"; do
  case "$arg" in
    --reset) RESET=1 ;;
    --reset-users) RESET_USERS=1 ;;
    *) echo "未知参数：$arg" ; exit 1 ;;
  esac
done

KV="cargo run --quiet -p kenvoy --"

# --- 重置（可选） ---
if [ "$RESET_USERS" = "1" ] || [ "$RESET" = "1" ]; then
  echo "重置 demo 数据..."
  # 通讯录按名字删（依赖 store 接口，下一轮增加 kv contact/del；目前用 SQL）
  # 简化处理：清空 demo 账户 + 通讯录关联
  PGPASSWORD=kenvoy psql -h 127.0.0.1 -p 5433 -U kenvoy -d kenvoy -v ON_ERROR_STOP=1 <<SQL
DELETE FROM contacts WHERE name IN ('Alice 张','Bob 李','Carol 王','Dan 赵');
DELETE FROM users WHERE username IN ('admin','hr','ceo','alice','bob','carol');
SQL
  echo "已重置。"
fi

# --- 角色 ---
ensure_role() {
  local name="$1" level="$2" desc="$3"
  if ! $KV role list 2>/dev/null | grep -q "$name"; then
    $KV role add --name "$name" --level "$level" --description "$desc" >/dev/null
    echo "  + 角色：$name (level=$level)"
  fi
}

echo "确保角色..."
ensure_role "管理员"     100 "系统管理员，全部权限"
ensure_role "HR"         80  "人事，通讯录/部门管理"
ensure_role "人事总监"   70  ">3 天请假审批、最终决策"
ensure_role "部门主管"   50  "部门请假审批（≤3 天）"
ensure_role "普通员工"   10  "提交请假、查看本人记录"

# --- 部门 ---
ensure_dept() {
  local name="$1" parent="$2"
  if ! $KV dept list 2>/dev/null | grep -q "$name"; then
    if [ -n "$parent" ]; then
      $KV dept add --name "$name" --parent "$parent" >/dev/null
    else
      $KV dept add --name "$name" >/dev/null
    fi
    echo "  + 部门：$name"
  fi
}

echo "确保部门..."
ensure_dept "技术中心"
ensure_dept "研发中心"   "技术中心"
ensure_dept "人事部"

# --- 账户 ---
ensure_user() {
  local username="$1" password="$2" display="$3" role="$4"
  if $KV user list 2>/dev/null | grep -q "$username"; then
    return 0
  fi
  $KV user add --username "$username" --password "$password" \
               --display-name "$display" --role "$role" >/dev/null
  echo "  + 账户：$username ($display, role=$role)"
}

echo "确保账户..."
ensure_user admin  demo123  "管理员"      "管理员"
ensure_user hr     demo123  "HR 小妹"     "HR"
ensure_user ceo    demo123  "陈总监"      "人事总监"
ensure_user alice  demo123  "Alice 张"    "部门主管"
ensure_user bob    demo123  "Bob 李"      "普通员工"

# --- 通讯录（无账号人员） ---
ensure_contact() {
  local name="$1" dept="$2" user="$3" email="$4" title="$5"
  if $KV contact list 2>/dev/null | grep -q "$name"; then
    return 0
  fi
  ARGS=( --name "$name" --title "$title" )
  [ -n "$dept" ] && ARGS+=( --department "$dept" )
  [ -n "$user" ] && ARGS+=( --user "$user" )
  [ -n "$email" ] && ARGS+=( --email "$email" )
  $KV contact add "${ARGS[@]}" >/dev/null
  echo "  + 通讯录：$name"
}

echo "确保通讯录..."
ensure_contact "Alice 张"  "研发中心"  "alice"  "alice@example.com"  "前端工程师"
ensure_contact "Bob 李"    "研发中心"  "bob"    "bob@example.com"    "后端工程师"
ensure_contact "Carol 王"  "人事部"    ""       "carol@example.com"  "HR 专员"
ensure_contact "Dan 赵"    "人事部"    ""       "dan@example.com"    "招聘经理"

echo ""
echo "✅ 引导完成。启动 dev："
echo "  cargo run -p kenvoy -- dev"
echo "打开 http://127.0.0.1:5173 用上面任一账户登录即可。"
