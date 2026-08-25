@echo off
REM examples\leave-app\seed.bat —— Windows 引导脚本（Git Bash / WSL 也可跑 .sh）
REM
REM 用法（在 examples\leave-app 目录）：
REM   seed.bat                          REM 完整 bootstrap
REM   seed.bat --reset                  REM 重置 demo 数据
REM
REM 演示账户（密码均为 demo123）：
REM   admin / demo123  等级 100  管理员
REM   hr    / demo123  等级 80   HR
REM   ceo   / demo123  等级 70   人事总监
REM   alice / demo123  等级 30   部门主管
REM   bob   / demo123  等级 10   普通员工
REM   carol 通讯录人员无账号

cd /d %~dp0

set RESET=0
if "%1"=="--reset" set RESET=1

if %RESET%==1 (
  echo 重置 demo 数据...
  set PGPASSWORD=kenvoy
  psql -h 127.0.0.1 -p 5433 -U kenvoy -d kenvoy -v ON_ERROR_STOP=1 <<SQL
DELETE FROM contacts WHERE name IN ('Alice 张','Bob 李','Carol 王','Dan 赵');
DELETE FROM users WHERE username IN ('admin','hr','ceo','alice','bob','carol');
SQL
)

REM 角色
call :ensure_role 管理员 100 系统管理员
call :ensure_role HR 80 人事
call :ensure_role 人事总监 70 ">3天请假审批"
call :ensure_role 部门主管 50 "≤3天请假审批"
call :ensure_role 普通员工 10 提交请假

REM 部门
call :ensure_dept 技术中心 ""
call :ensure_dept 研发中心 技术中心
call :ensure_dept 人事部 ""

REM 账户
call :ensure_user admin demo123 管理员 管理员
call :ensure_user hr demo123 HR HR
call :ensure_user ceo demo123 陈总监 人事总监
call :ensure_user alice demo123 "Alice 张" 部门主管
call :ensure_user bob demo123 "Bob 李" 普通员工

REM 通讯录
call :ensure_contact "Alice 张" 研发中心 alice "alice@example.com" 前端工程师
call :ensure_contact "Bob 李" 研发中心 bob "bob@example.com" 后端工程师
call :ensure_contact "Carol 王" 人事部 "" "carol@example.com" HR专员
call :ensure_contact "Dan 赵" 人事部 "" "dan@example.com" 招聘经理

echo.
echo ✅ 引导完成。启动 dev：
echo   cargo run -p kenvoy -- dev
echo 打开 http://127.0.0.1:5173 用任一账户登录。
exit /b 0

:ensure_role
cargo run --quiet -p kenvoy -- role list | findstr /C:"%~1" >nul
if errorlevel 1 (
  cargo run --quiet -p kenvoy -- role add --name "%~1" --level %~2 --description "%~3" >nul
  echo   + 角色：%~1
)
exit /b 0

:ensure_dept
cargo run --quiet -p kenvoy -- dept list | findstr /C:"%~1" >nul
if errorlevel 1 (
  if "%~2"=="" (
    cargo run --quiet -p kenvoy -- dept add --name "%~1" >nul
  ) else (
    cargo run --quiet -p kenvoy -- dept add --name "%~1" --parent "%~2" >nul
  )
  echo   + 部门：%~1
)
exit /b 0

:ensure_user
cargo run --quiet -p kenvoy -- user list | findstr /C:"%~1" >nul
if errorlevel 1 (
  cargo run --quiet -p kenvoy -- user add --username "%~1" --password "%~2" --display-name "%~3" --role "%~4" >nul
  echo   + 账户：%~1
)
exit /b 0

:ensure_contact
cargo run --quiet -p kenvoy -- contact list | findstr /C:"%~1" >nul
if errorlevel 1 (
  cargo run --quiet -p kenvoy -- contact add --name "%~1" --department "%~2" --user "%~3" --email "%~4" --title "%~5" >nul
  echo   + 通讯录：%~1
)
exit /b 0
