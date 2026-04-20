import "./dialog.css";

export function ensureDialogStyles(): void {
	// Styles are injected via CSS import (dialog.css)
}

export function showErrorDialog(message: string): void {
	ensureDialogStyles();
	const overlay = document.createElement("div");
	overlay.className = "zh-overlay";
	const dialog = document.createElement("div");
	dialog.className = "zh-dialog";
	dialog.style.minWidth = "320px";
	dialog.style.maxWidth = "520px";
	const title = document.createElement("h3");
	title.style.color = "var(--accent, #e94560)";
	title.textContent = "错误";
	dialog.appendChild(title);
	const msg = document.createElement("p");
	msg.style.cssText = "margin:0 0 16px;font-size:13px;white-space:pre-wrap;word-break:break-all;line-height:1.5;";
	msg.textContent = message;
	dialog.appendChild(msg);
	const btns = document.createElement("div");
	btns.className = "zh-buttons";
	const okBtn = document.createElement("button");
	okBtn.textContent = "确定";
	okBtn.className = "primary";
	const close = () => overlay.remove();
	okBtn.addEventListener("click", close);
	btns.appendChild(okBtn);
	dialog.appendChild(btns);
	overlay.appendChild(dialog);
	overlay.addEventListener("click", (e) => {
		if (e.target === overlay) close();
	});
	document.body.appendChild(overlay);
	setTimeout(() => okBtn.focus(), 0);
}

export function openOverlay(): { overlay: HTMLDivElement; dialog: HTMLDivElement; close: () => void } {
	ensureDialogStyles();
	const overlay = document.createElement("div");
	overlay.className = "zh-overlay";
	const dialog = document.createElement("div");
	dialog.className = "zh-dialog";
	overlay.appendChild(dialog);
	document.body.appendChild(overlay);
	const close = () => overlay.remove();
	return { overlay, dialog, close };
}
