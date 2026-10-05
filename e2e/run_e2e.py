#!/usr/bin/env python3
"""End-to-end smoke test of the real PrintFold app (Linux / WebKitGTK).

Drives the Tauri binary through tauri-driver + WebKitWebDriver: creates a
project, adds markdown and an image, waits for the Rust reflow, edits
options, adds static-page items (and pastes one from the context menu),
previews and exports the PDF, then reopens the auto-saved project.

Native dialogs are bypassed with the PRINTFOLD_E2E_DIR / PRINTFOLD_E2E_PICK
hooks (see src-tauri/src/platform.rs). Screenshots go to target/e2e/.

Requirements: tauri-driver (cargo install tauri-driver), WebKitWebDriver
(webkit2gtk-driver), Xvfb, python3-selenium, and a built app:
    npx tauri build --debug --no-bundle
"""

import os
import shutil
import subprocess
import sys
import json
import tempfile
import time
import zipfile
from pathlib import Path

from selenium import webdriver
from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.common.by import By
from selenium.webdriver.support.ui import WebDriverWait

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "target" / "debug" / "printfold"
FIXTURES = Path(__file__).resolve().parent / "fixtures"
SHOTS = ROOT / "target" / "e2e"


def log(msg):
    print(f"[e2e] {msg}", flush=True)


def start_display():
    if os.environ.get("DISPLAY"):
        return None
    xvfb = subprocess.Popen(["Xvfb", ":99", "-screen", "0", "1600x1000x24"],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    os.environ["DISPLAY"] = ":99"
    time.sleep(1)
    return xvfb


class Run:
    def __init__(self, workdir):
        self.workdir = workdir
        self.driver = None

    def launch(self):
        options = webdriver.ChromeOptions()  # any Options carrying capabilities works
        options.set_capability("browserName", "wry")
        options.set_capability("tauri:options", {"application": str(APP)})
        self.driver = webdriver.Remote(command_executor="http://127.0.0.1:4444", options=options)
        self.driver.set_window_size(1500, 950)
        return self.driver

    def js(self, script, *args):
        return self.driver.execute_script(script, *args)

    def wait(self, predicate, timeout=30, what="condition"):
        try:
            return WebDriverWait(self.driver, timeout).until(lambda d: predicate())
        except Exception:
            self.shot(f"timeout-{what.replace(' ', '-')}")
            raise AssertionError(f"timed out waiting for {what}")

    def shot(self, name):
        SHOTS.mkdir(parents=True, exist_ok=True)
        path = SHOTS / f"{name}.png"
        self.driver.save_screenshot(str(path))
        log(f"screenshot {path.relative_to(ROOT)}")

    def click(self, selector):
        self.driver.find_element(By.CSS_SELECTOR, selector).click()

    def visible(self, selector):
        return self.js(
            "const e=document.querySelector(arguments[0]);"
            "if (!e || e.classList.contains('hidden')) return false;"
            "const r=e.getBoundingClientRect(); return getComputedStyle(e).display!=='none' && r.width>0 && r.height>0;", selector)

    def text(self, selector):
        return self.js("const e=document.querySelector(arguments[0]); return e ? e.textContent : null;", selector)


def main():
    if not APP.exists():
        sys.exit(f"build the app first: {APP} not found")
    xvfb = start_display()
    workdir = Path(tempfile.mkdtemp(prefix="printfold-e2e-"))
    picks = "|".join(str(FIXTURES / f) for f in ("sample.md", "picture.png"))
    env = dict(os.environ, PRINTFOLD_E2E_DIR=str(workdir), PRINTFOLD_E2E_PICK=picks,
               XDG_DATA_HOME=str(workdir / "xdg"))
    os.environ.update(env)
    driver_proc = subprocess.Popen(["tauri-driver"], env=env,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(2)
    run = Run(workdir)
    failures = []
    try:
        run.launch()
        run.wait(lambda: run.visible("#welcome-screen"), what="welcome screen")
        run.shot("01-welcome")

        # New project → editor
        run.click("#welcome-new")
        run.wait(lambda: not run.visible("#welcome-screen"), what="editor")
        project_files = list(workdir.glob("*.printfold"))
        assert project_files, "new project file was not created"
        log(f"project file: {project_files[0].name}")

        # Add markdown + image via the Files "+" button (picker hook)
        run.click("#btn-add-files")
        run.wait(lambda: run.js("return document.querySelectorAll('.file-item').length") >= 1,
                 what="file list")

        # Wait for reflow: more than the default 4 pages laid out with text
        run.wait(lambda: int(run.text("#info-pages") or 0) >= 8, timeout=60, what="reflow")
        pages = int(run.text("#info-pages"))
        log(f"pages after reflow: {pages}")
        time.sleep(1.5)
        run.shot("02-editor")

        # Change a layout option (pages per signature) and check it reflows
        run.js("""
            const sel = document.getElementById('opt-pages-per-sig');
            if (sel) { sel.value = '8'; sel.dispatchEvent(new Event('change', {bubbles: true})); }
        """)
        time.sleep(2)
        sigs = int(run.text("#info-signatures") or 0)
        log(f"signatures after option change: {sigs}")

        # Preview tab renders PDF pages with pdf.js
        run.click('.tab[data-tab="preview"]')
        run.wait(lambda: run.js("return document.querySelectorAll('.pdf-page canvas').length") >= 1,
                 timeout=90, what="pdf preview")
        time.sleep(1)
        run.shot("03-preview")
        run.click('.tab[data-tab="editor"]')

        # Export PDF (save hook writes to the work dir)
        run.click("#btn-export")
        pdf_path = workdir / f"{project_files[0].stem}.pdf"
        run.wait(lambda: pdf_path.exists() and pdf_path.stat().st_size > 1000, timeout=90, what="pdf export")
        info = subprocess.run(["pdfinfo", str(pdf_path)], capture_output=True, text=True).stdout
        log("exported PDF: " + " | ".join(l for l in info.splitlines() if l.startswith(("Pages", "Page size"))))
        subprocess.run(["pdftoppm", "-r", "50", "-png", "-f", "1", "-l", "2", str(pdf_path), str(SHOTS / "04-export")])

        # Static page + item: tap the recto of spread 1 (page 1), insert a
        # static page there, add a rectangle and a text item.
        run.click("#btn-prev-spread")
        container = run.driver.find_element(By.CSS_SELECTOR, "#konva-container")
        ActionChains(run.driver).move_to_element_with_offset(container, 150, 0).click().perform()
        run.wait(lambda: run.js("return getComputedStyle(document.getElementById('toolbar-add-items')).display") != "none",
                 what="page selection")
        run.click("#btn-add-single-page")
        time.sleep(1.5)
        ActionChains(run.driver).move_to_element_with_offset(container, 150, 0).click().perform()
        time.sleep(0.5)
        run.click("#btn-add-rect")
        time.sleep(0.5)
        run.click("#btn-add-text")
        time.sleep(0.5)
        # Text-flow region: the markdown flow continues inside it
        run.click("#btn-add-text-flow")
        time.sleep(2.5)
        run.shot("04b-static-page")

        # Copy an item, then paste it from the context menu on empty page
        # space (the only paste route on an iPad without a keyboard).
        # WebKitWebDriver's context click does not raise a DOM contextmenu
        # event, so that event is dispatched directly.
        def stage_rect(predicate_js):
            return run.js("""
                const stage = Konva.stages[0];
                const node = stage.find(""" + predicate_js + """)[0];
                const r = node.getClientRect();
                return [r.x, r.y, r.width, r.height, stage.width(), stage.height()];
            """)
        x, y, rw, rh, w, h = stage_rect("n => n.getAttr('itemId') && n.getClassName() === 'Rect'")
        ActionChains(run.driver).move_to_element_with_offset(container, int(x + rw / 2 - w / 2), int(y + rh / 2 - h / 2)).click().perform()
        time.sleep(0.5)
        run.js("document.dispatchEvent(new KeyboardEvent('keydown', {key: 'c', ctrlKey: true, metaKey: true}))")
        x, y, rw, rh, w, h = stage_rect("n => n.getAttr('clickPagePosition') === 'recto'")
        run.js("""
            const content = document.querySelector('#konva-container .konvajs-content');
            const r = content.getBoundingClientRect();
            content.dispatchEvent(new MouseEvent('contextmenu', {
                clientX: r.left + arguments[0], clientY: r.top + arguments[1], button: 2, bubbles: true, cancelable: true }));
        """, x + rw - 12, y + rh - 12)
        run.wait(lambda: run.js("return [...document.querySelectorAll('.context-menu div')].some(d => d.textContent === 'Paste')"),
                 what="paste menu")
        run.js("[...document.querySelectorAll('.context-menu div')].find(d => d.textContent === 'Paste').click()")
        time.sleep(1)
        # Back to page selection (empty page space) for the page export below.
        ActionChains(run.driver).move_to_element_with_offset(container, int(x + rw - 12 - w / 2), int(y + rh - 12 - h / 2)).click().perform()
        time.sleep(0.5)

        # PNG export of the static page from the Selected tab
        run.js("document.querySelector('.options-tabs .tab-btn[data-tab=\"selected\"]').click()")
        time.sleep(0.5)
        has_download = run.js("const b=document.getElementById('btn-download-current'); return !!b && b.offsetParent!==null")
        if has_download:
            run.click("#btn-download-current")
            run.wait(lambda: any(workdir.glob("page-*.png")), timeout=60, what="page png export")
            log(f"page export: {[p.name for p in workdir.glob('page-*.png')]}")
        else:
            log("download-current button not visible (page not static?)")

        # Re-export: the static page's items arrive as a pre-rendered image
        pdf_path.unlink()
        run.click("#btn-export")
        run.wait(lambda: pdf_path.exists() and pdf_path.stat().st_size > 1000, timeout=90, what="pdf re-export")
        images = subprocess.run(["pdfimages", "-list", str(pdf_path)], capture_output=True, text=True).stdout
        image_rows = [l for l in images.splitlines()[2:] if l.strip()]
        log(f"images in PDF after adding items: {len(image_rows)}")
        assert len(image_rows) >= 2, "expected the picture plus a pre-rendered static page"
        subprocess.run(["pdftoppm", "-r", "50", "-png", "-f", "1", "-l", "2", str(pdf_path), str(SHOTS / "04c-export-items")])

        # Duplex test page from the Output tab
        run.js("document.querySelector('.options-tabs .tab-btn[data-tab=\"output\"]').click()")
        time.sleep(0.3)
        run.click("#btn-print-test-page")
        test_pdf = workdir / "duplex-test-page.pdf"
        run.wait(lambda: test_pdf.exists() and test_pdf.stat().st_size > 500, timeout=60, what="test page")
        log("duplex test page exported")

        # Auto-save wrote a real project archive
        time.sleep(2)
        project = project_files[0]
        with zipfile.ZipFile(project) as z:
            names = z.namelist()
        assert "project.json" in names and any(n.startswith("text/") for n in names), names
        assert any(n.startswith("images/") for n in names), names
        log(f"autosaved archive: {sorted(names)}")
        with zipfile.ZipFile(project) as z:
            item_count = sum(len(json.loads(z.read(n)).get("items", [])) for n in names if n.startswith("static/") and n.endswith(".json"))
        log(f"items on static pages: {item_count}")
        assert item_count >= 4, "expected rectangle, text, text-flow region and the pasted copy"

        # Reopen from the welcome screen's recents
        run.click("#btn-welcome")
        run.wait(lambda: run.visible("#welcome-screen"), what="welcome again")
        run.shot("05-recents")
        run.wait(lambda: run.js("return document.querySelectorAll('.recent-row').length") >= 1, what="recents")
        run.click(".recent-row")
        run.wait(lambda: not run.visible("#welcome-screen"), what="reopened editor")
        run.wait(lambda: int(run.text("#info-pages") or 0) >= 8, timeout=60, what="reflow after reopen")
        log(f"pages after reopen: {run.text('#info-pages')}")
        run.shot("06-reopened")

        # Narrow window (iPad portrait-like) layout
        run.driver.set_window_size(900, 1150)
        time.sleep(1.5)
        run.shot("07-narrow")
    except AssertionError as e:
        failures.append(str(e))
        log(f"FAIL: {e}")
    finally:
        try:
            logs = run.driver.get_log("browser") if run.driver else []
            for entry in logs[-30:]:
                log(f"console: {entry}")
        except Exception:
            pass
        if run.driver:
            run.driver.quit()
        driver_proc.terminate()
        if xvfb:
            xvfb.terminate()
        shutil.rmtree(workdir, ignore_errors=True)
    if failures:
        sys.exit(1)
    log("all checks passed")


if __name__ == "__main__":
    main()
