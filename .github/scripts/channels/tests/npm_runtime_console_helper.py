"""只控制自己创建的隐藏 Windows 控制台，验证包装器不会强杀处理 Ctrl+C 的子进程。"""

import ctypes
import json
import queue
import subprocess
import sys
import threading
import time


def send_interrupt(pid):
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.FreeConsole()
    if not kernel.AttachConsole(pid):
        raise OSError(ctypes.get_last_error(), "无法附加自有测试控制台")
    try:
        kernel.SetConsoleCtrlHandler(None, True)
        if not kernel.GenerateConsoleCtrlEvent(0, 0):
            raise OSError(ctypes.get_last_error(), "无法发送控制台中断")
        # 控制台事件异步分发，辅助进程短暂保留连接后退出。
        time.sleep(0.2)
    finally:
        kernel.FreeConsole()


def run(command):
    # Node 测试宿主可能忽略 Ctrl+C；仅重置辅助进程自己的可继承标记。
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    if not kernel.SetConsoleCtrlHandler(None, False):
        raise OSError(ctypes.get_last_error(), "无法设置测试控制台中断策略")
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    child = subprocess.Popen(
        command, creationflags=subprocess.CREATE_NEW_CONSOLE, startupinfo=startup,
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True, encoding="utf-8",
    )
    try:
        ready = queue.Queue()
        threading.Thread(target=lambda: ready.put(child.stdout.readline()), daemon=True).start()
        if ready.get(timeout=5).strip() != "ready":
            raise RuntimeError("自有测试子进程未就绪")
        signal = subprocess.run(
            [sys.executable, __file__, "--signal", str(child.pid)],
            capture_output=True, text=True, encoding="utf-8", timeout=5,
        )
        if signal.returncode:
            raise RuntimeError(signal.stderr)
        output, error = child.communicate(timeout=5)
        print(json.dumps({"code": child.returncode, "stdout": output, "stderr": error}))
    finally:
        if child.poll() is None:
            subprocess.run(
                ["taskkill", "/PID", str(child.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
            )


if __name__ == "__main__":
    if sys.argv[1] == "--signal":
        send_interrupt(int(sys.argv[2]))
    else:
        run(sys.argv[1:])
