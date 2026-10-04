/**
 * In-app alert/prompt dialogs built on the shared modal (`#modal-overlay`).
 *
 * `window.alert/prompt` are unreliable inside WKWebView (iPadOS in
 * particular), so the app uses its own modal for messages and text input.
 */

function modalParts() {
  const overlay = document.getElementById('modal-overlay');
  const title = document.getElementById('modal-title');
  const content = document.getElementById('modal-content');
  const confirmBtn = document.getElementById('modal-confirm');
  const cancelBtn = document.getElementById('modal-cancel');
  const closeBtn = overlay?.querySelector('.modal-close') as HTMLElement | null;
  if (!overlay || !title || !content || !confirmBtn || !cancelBtn) return null;
  // Replace buttons to drop handlers left by other modal users.
  const freshConfirm = confirmBtn.cloneNode(true) as HTMLButtonElement;
  const freshCancel = cancelBtn.cloneNode(true) as HTMLButtonElement;
  confirmBtn.replaceWith(freshConfirm);
  cancelBtn.replaceWith(freshCancel);
  let freshClose: HTMLElement | null = null;
  if (closeBtn) {
    freshClose = closeBtn.cloneNode(true) as HTMLElement;
    closeBtn.replaceWith(freshClose);
  }
  return { overlay, title, content, confirm: freshConfirm, cancel: freshCancel, close: freshClose };
}

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

/** Show a message with an OK button. */
export function showAlert(message: string, heading = 'PrintFold'): Promise<void> {
  const parts = modalParts();
  if (!parts) {
    console.warn(message);
    return Promise.resolve();
  }
  const { overlay, title, content, confirm, cancel, close } = parts;
  title.textContent = heading;
  content.innerHTML = `<p class="modal-message">${escapeHtml(message)}</p>`;
  cancel.style.display = 'none';
  confirm.textContent = 'OK';
  overlay.classList.remove('hidden');
  return new Promise(resolve => {
    const done = () => {
      overlay.classList.add('hidden');
      cancel.style.display = '';
      resolve();
    };
    confirm.addEventListener('click', done, { once: true });
    close?.addEventListener('click', done, { once: true });
    confirm.focus();
  });
}

/** Ask for a line of text; resolves to null if cancelled. */
export function showPrompt(heading: string, label: string, initial = '', okLabel = 'OK'): Promise<string | null> {
  const parts = modalParts();
  if (!parts) return Promise.resolve(initial);
  const { overlay, title, content, confirm, cancel, close } = parts;
  title.textContent = heading;
  content.innerHTML = `
    <label class="modal-field">
      <span>${escapeHtml(label)}</span>
      <input type="text" id="modal-prompt-input" autocomplete="off" autocapitalize="words" />
    </label>`;
  const input = content.querySelector('#modal-prompt-input') as HTMLInputElement;
  input.value = initial;
  confirm.textContent = okLabel;
  overlay.classList.remove('hidden');
  return new Promise(resolve => {
    const finish = (value: string | null) => {
      overlay.classList.add('hidden');
      confirm.textContent = 'OK';
      resolve(value);
    };
    confirm.addEventListener('click', () => finish(input.value.trim() || initial), { once: true });
    cancel.addEventListener('click', () => finish(null), { once: true });
    close?.addEventListener('click', () => finish(null), { once: true });
    input.addEventListener('keydown', e => {
      if (e.key === 'Enter') confirm.click();
      if (e.key === 'Escape') cancel.click();
    });
    setTimeout(() => {
      input.focus();
      input.select();
    }, 50);
  });
}
