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

    def menu(self, label):
        """Click an entry of the open context/dropdown menu."""
        self.wait(lambda: self.js("return [...document.querySelectorAll('.context-menu div')].some(d => d.textContent === arguments[0])", label),
                  what=f"menu item {label}")
        self.js("[...document.querySelectorAll('.context-menu div')].find(d => d.textContent === arguments[0]).click()", label)

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
        run.wait(lambda: run.visible("#welcome-screen"), what="project browser")
        run.shot("01-browser-empty")

        # New project → editor
        run.click("#browser-new")
        run.wait(lambda: not run.visible("#welcome-screen"), what="editor")
        project_files = list(workdir.glob("*.printfold"))
        assert project_files, "new project file was not created"
        log(f"project file: {project_files[0].name}")

        # Rename from the header (click the project name)
        run.click("#project-name-display")
        run.wait(lambda: run.js("return !!document.getElementById('modal-prompt-input')"), what="rename prompt")
        run.js("const i = document.getElementById('modal-prompt-input'); i.value = 'Field Guide';")
        run.click("#modal-confirm")
        run.wait(lambda: (workdir / "Field Guide.printfold").exists(), what="renamed project file")
        assert run.text("#project-name-display") == "Field Guide", run.text("#project-name-display")
        log("renamed from the header: Field Guide.printfold")

        # Add markdown + image via the Files "+" button (picker hook)
        run.click("#btn-add-files")
        run.menu("Import File…")
        run.wait(lambda: run.js("return document.querySelectorAll('.file-item').length") >= 1,
                 what="file list")

        # Drop a .txt file from "another app" onto the Files panel: it
        # becomes a markdown file.
        run.js("""
            const dt = new DataTransfer();
            dt.items.add(new File(['Dropped notes.'], 'notes.txt', {type: 'text/plain'}));
            const list = document.getElementById('file-list');
            for (const type of ['dragenter', 'dragover', 'drop']) {
                list.dispatchEvent(new DragEvent(type, {dataTransfer: dt, bubbles: true, cancelable: true}));
            }
        """)
        run.wait(lambda: run.js("return [...document.querySelectorAll('.file-item')].some(e => e.textContent.includes('notes.md'))"),
                 what="dropped text file")
        log("dropped notes.txt → notes.md")

        # + › From Clipboard: text becomes a markdown file named after its
        # first line; a PNG becomes an image file.
        def set_clipboard(data, mime):
            subprocess.run(["xclip", "-selection", "clipboard", "-t", mime, "-i"], input=data, check=True)
        set_clipboard(b"# Clipboard Chapter\n\nPasted from the clipboard.\n", "UTF8_STRING")
        run.click("#btn-add-files")
        run.menu("From Clipboard")
        run.wait(lambda: run.js("return [...document.querySelectorAll('.file-item')].some(e => e.textContent.includes('Clipboard Chapter.md'))"),
                 what="markdown file from clipboard")
        set_clipboard((FIXTURES / "picture.png").read_bytes(), "image/png")
        run.click("#btn-add-files")
        run.menu("From Clipboard")
        run.wait(lambda: run.js("return [...document.querySelectorAll('.file-item')].some(e => e.textContent.includes('Clipboard image.png'))"),
                 what="image file from clipboard")
        run.js("[...document.querySelectorAll('.file-tab')].find(t => t.textContent.startsWith('Text')).click()")
        log("from clipboard: Clipboard Chapter.md, Clipboard image.png")

        # The markdown editor opens at half the column's height.
        run.js("document.querySelector('.file-item .btn-edit-file').click()")
        run.wait(lambda: run.js("return !document.querySelector('.panel-preview').classList.contains('collapsed')"), what="editor panel")
        time.sleep(0.5)
        ratio = run.js("""
            const files = document.querySelector('.panel-files').getBoundingClientRect().height;
            const editor = document.querySelector('.panel-preview').getBoundingClientRect().height;
            return editor / (files + editor);
        """)
        assert 0.4 < ratio < 0.6, f"editor takes {ratio:.2f} of the column"
        log(f"markdown editor opens at {ratio:.0%} of the column")
        run.click("#btn-close-preview")

        # Wait for reflow: more than the default 4 pages laid out with text
        run.wait(lambda: int(run.text("#info-pages") or 0) >= 8, timeout=60, what="reflow")
        pages = int(run.text("#info-pages"))
        log(f"pages after reflow: {pages}")
        time.sleep(1.5)
        run.shot("02-editor")

        # Dragging a margin guide moves the facing page's guide too (live).
        run.js("const c = document.getElementById('chk-show-margins'); if (c && !c.checked) c.click();")
        run.click("#btn-next-spread")
        time.sleep(1)
        def top_guides():
            return run.js("""
                const lines = Konva.stages[0].find(n => n.getClassName() === 'Line' && n.hitStrokeWidth() === 20)
                  .map(n => { const p = n.points(); const t = n.getAbsoluteTransform().point({x: p[0], y: p[1]});
                              const e = n.getAbsoluteTransform().point({x: p[2], y: p[3]}); return [t.x, t.y, e.x, e.y]; })
                  .filter(p => Math.abs(p[1] - p[3]) < 0.5);
                const top = Math.min(...lines.map(p => p[1]));
                return lines.filter(p => Math.abs(p[1] - top) < 1).sort((a, b) => a[0] - b[0]);
            """)
        guides = top_guides()
        assert len(guides) == 2, f"expected a top guide on both pages, got {guides}"
        rx, ry = (guides[1][0] + guides[1][2]) / 2, guides[1][1]
        cw, ch = run.js("const s = Konva.stages[0]; return [s.width(), s.height()]")
        actions = ActionChains(run.driver)
        actions.move_to_element_with_offset(container_el := run.driver.find_element(By.CSS_SELECTOR, "#konva-container"),
                                            int(rx - cw / 2), int(ry - ch / 2)).click_and_hold().move_by_offset(0, 12).move_by_offset(0, 12).perform()
        time.sleep(0.3)
        during = top_guides() or []
        ActionChains(run.driver).release().perform()
        moved = [round(d[1] - g[1]) for d, g in zip(sorted(during, key=lambda p: p[0]), guides)] if len(during) == 2 else []
        log(f"margin drag: guide moves during drag {moved}")
        assert len(moved) == 2 and moved[0] > 10 and moved[1] > 10, f"facing guide did not follow the drag: {moved}"
        time.sleep(1)
        run.click("#btn-prev-spread")

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
        # Thumbnails beside the pages; the page column scrolls, and a
        # thumbnail click jumps to its page.
        thumbs = run.js("return document.querySelectorAll('.pdf-thumb').length")
        pages_in_preview = run.js("return document.querySelectorAll('.pdf-page').length")
        assert thumbs == pages_in_preview and thumbs >= 2, (thumbs, pages_in_preview)
        scrollable = run.js("const c = document.getElementById('pdf-preview-container'); return c.scrollHeight > c.clientHeight")
        assert scrollable, "preview pages do not scroll"
        run.js(f"document.querySelectorAll('.pdf-thumb')[{thumbs - 1}].click()")
        run.wait(lambda: run.js("return document.getElementById('pdf-preview-container').scrollTop") > 0, what="thumbnail navigation")
        run.wait(lambda: run.js("return document.querySelector('.pdf-thumb.current')?.dataset.page") == str(thumbs), what="current thumbnail")
        log(f"preview: {thumbs} pages with thumbnails, scrolls")
        time.sleep(0.6)
        run.shot("03-preview")
        run.click('.tab[data-tab="editor"]')

        # Export PDF (save hook writes to the work dir)
        run.click("#btn-export")
        pdf_path = workdir / "Field Guide.pdf"
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
        # Drop an image file from "another app" onto the static page.
        payload_png = __import__("base64").b64encode((FIXTURES / "picture.png").read_bytes()).decode()
        run.js("""
            const bytes = Uint8Array.from(atob(arguments[0]), c => c.charCodeAt(0));
            const dt = new DataTransfer();
            dt.items.add(new File([bytes], 'dropped-photo.png', {type: 'image/png'}));
            const content = document.querySelector('#konva-container');
            const r = content.getBoundingClientRect();
            for (const type of ['dragenter', 'dragover', 'drop']) {
                content.dispatchEvent(new DragEvent(type, {dataTransfer: dt, bubbles: true, cancelable: true,
                    clientX: r.left + arguments[1], clientY: r.top + arguments[2]}));
            }
        """, payload_png, x + rw / 2, y + rh / 2)
        run.wait(lambda: run.js("return Konva.stages[0].find(n => n.getClassName() === 'Image' && n.getAttr('itemId')).length") >= 1,
                 what="dropped image placed")
        log("dropped an image file onto the static page")
        time.sleep(0.5)

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

        # Add Background › From Clipboard sets the page's custom background.
        set_clipboard((FIXTURES / "picture.png").read_bytes(), "image/png")
        run.js("document.querySelector('.options-tabs .tab-btn[data-tab=\"selected\"]').click()")
        run.wait(lambda: run.visible("#btn-upload-background"), what="Add Background button")
        assert run.text("#btn-upload-background") == "Add Background"
        run.click("#btn-upload-background")
        run.menu("From Clipboard")
        run.wait(lambda: run.js("const b = document.getElementById('btn-remove-background'); return !!b && b.style.display !== 'none'"),
                 what="background from clipboard")
        log("Add Background › From Clipboard")

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
        project = workdir / "Field Guide.printfold"
        with zipfile.ZipFile(project) as z:
            names = z.namelist()
        assert "project.json" in names and any(n.startswith("text/") for n in names), names
        assert any(n.startswith("images/") for n in names), names
        log(f"autosaved archive: {sorted(names)}")
        with zipfile.ZipFile(project) as z:
            item_count = sum(len(json.loads(z.read(n)).get("items", [])) for n in names if n.startswith("static/") and n.endswith(".json"))
        log(f"items on static pages: {item_count}")
        assert item_count >= 5, "expected rectangle, text, text-flow region, the pasted copy and the dropped image"

        # Back to the project browser: the project is saved with a cover
        # thumbnail and shown as a card.
        run.click("#btn-welcome")
        run.wait(lambda: run.visible("#welcome-screen"), what="project browser again")
        run.wait(lambda: run.js("return document.querySelectorAll('.project-card').length") == 1, what="one card")
        run.wait(lambda: run.js("return !!document.querySelector('.project-card .card-page img')"), what="thumbnail")
        with zipfile.ZipFile(project) as z:
            assert "preview/thumbnail" in z.namelist(), "thumbnail missing from archive"
        time.sleep(0.5)
        run.shot("05-browser")

        def cards():
            return run.js("return [...document.querySelectorAll('.project-card .card-name')].map(n => n.textContent)")

        def card_selector(name):
            return run.js("""
                const c = [...document.querySelectorAll('.project-card')].find(c => c.querySelector('.card-name').textContent === arguments[0]);
                return c ? '.project-card[data-path="' + CSS.escape(c.dataset.path) + '"]' : null;
            """, name)

        def long_press(selector):
            el = run.driver.find_element(By.CSS_SELECTOR, selector)
            ActionChains(run.driver).click_and_hold(el).pause(0.8).release().perform()

        # Long press selects; Duplicate from the selection toolbar
        long_press(card_selector("Field Guide") + " .card-thumb")
        run.wait(lambda: run.js("return document.getElementById('browser-toolbar').classList.contains('active')"),
                 what="selection toolbar")
        time.sleep(0.4)  # toolbar slides open
        run.click('[data-browser-action="duplicate"]')
        run.wait(lambda: "Field Guide copy" in cards(), what="duplicate")
        assert (workdir / "Field Guide copy.printfold").exists()

        # Inline rename from the selection toolbar (select only the copy)
        run.click('[data-browser-action="done"]')
        long_press(card_selector("Field Guide copy") + " .card-thumb")
        time.sleep(0.4)
        run.click('[data-browser-action="rename"]')
        run.wait(lambda: run.js("return !!document.querySelector('.card-rename')"), what="inline rename field")
        run.js("""
            const i = document.querySelector('.card-rename'); i.value = 'Draft Two';
            i.dispatchEvent(new KeyboardEvent('keydown', {key: 'Enter', bubbles: true}));
        """)
        run.wait(lambda: (workdir / "Draft Two.printfold").exists(), what="inline rename")
        run.wait(lambda: "Draft Two" in cards(), what="renamed card")

        # Delete with the keyboard (confirmation dialog)
        run.js("document.querySelector('[data-browser-action=\"done\"]').click()")
        long_press(card_selector("Draft Two") + " .card-thumb")
        run.js("document.dispatchEvent(new KeyboardEvent('keydown', {key: 'Backspace', bubbles: true}))")
        run.wait(lambda: run.visible("#modal-confirm"), what="delete confirmation")
        run.click("#modal-confirm")
        run.wait(lambda: not (workdir / "Draft Two.printfold").exists(), what="delete")
        run.wait(lambda: cards() == ["Field Guide"], what="card removed")

        # Drop a .printfold onto the browser to import it
        import base64
        payload = base64.b64encode(project.read_bytes()).decode()
        run.js("""
            const bytes = Uint8Array.from(atob(arguments[0]), c => c.charCodeAt(0));
            const dt = new DataTransfer();
            dt.items.add(new File([bytes], 'Dropped.printfold'));
            const root = document.getElementById('welcome-screen');
            for (const type of ['dragenter', 'dragover', 'drop']) {
                root.dispatchEvent(new DragEvent(type, {dataTransfer: dt, bubbles: true, cancelable: true}));
            }
        """, payload)
        run.wait(lambda: (workdir / "Dropped.printfold").exists(), what="drop import")
        run.wait(lambda: "Dropped" in cards(), what="dropped card")
        log(f"browser operations ok: {sorted(cards())}")

        # A single click opens
        run.js("document.querySelector('[data-browser-action=\"done\"]').click()")
        run.click(card_selector("Field Guide") + " .card-thumb")
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
