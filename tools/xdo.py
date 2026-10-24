#!/usr/bin/env python3
"""Drive the running 3dLoom on the virtual display :90 with xdotool.

No window manager runs on :90, so keyboard events need an explicit
`windowfocus` first; this helper does that once and exposes click/key.
Usage from tests/scripts: from tools.xdo import Xdo; x = Xdo(); x.click(55, 33); x.key("Escape")
"""
import os, subprocess, time

class Xdo:
    def __init__(self, display=":90", title="3dLoom"):
        self.env = dict(os.environ, DISPLAY=display)
        for _ in range(50):
            ids = subprocess.run(["xdotool", "search", "--name", title], capture_output=True, text=True, env=self.env).stdout.split()
            if ids:
                self.wid = ids[-1]
                break
            time.sleep(0.2)
        else:
            raise RuntimeError("3dLoom window not found on " + display)
        subprocess.run(["xdotool", "windowfocus", "--sync", self.wid], env=self.env)

    def _run(self, *args):
        subprocess.run(["xdotool", *args], env=self.env, check=True)

    def move(self, x, y):
        self._run("mousemove", str(x), str(y))

    def click(self, x, y, button=1):
        self._run("mousemove", str(x), str(y), "click", str(button)); time.sleep(0.25)

    def drag(self, x0, y0, x1, y1, button=1, steps=10, keys=None):
        if keys: self._run("keydown", keys)
        self._run("mousemove", str(x0), str(y0), "mousedown", str(button))
        for i in range(1, steps + 1):
            self._run("mousemove", str(x0 + (x1 - x0) * i // steps), str(y0 + (y1 - y0) * i // steps)); time.sleep(0.02)
        self._run("mouseup", str(button))
        if keys: self._run("keyup", keys)
        time.sleep(0.25)

    def key(self, chord):
        self._run("key", chord); time.sleep(0.25)

    def type(self, text):
        self._run("type", text); time.sleep(0.25)
