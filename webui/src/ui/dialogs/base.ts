const DIALOG_STYLES = `
	.zh-overlay {
		position: fixed; inset: 0; background: rgba(0,0,0,0.6);
		display: flex; align-items: center; justify-content: center;
		z-index: 9999; font-family: sans-serif;
	}
	.zh-dialog {
		background: var(--bg); border: 1px solid var(--border); border-radius: 8px;
		padding: 20px; min-width: 480px; max-width: 720px; max-height: 80vh;
		overflow-y: auto; color: var(--text); box-shadow: 0 8px 32px rgba(0,0,0,0.5);
	}
	.zh-dialog h3 { margin: 0 0 12px; font-size: 15px; color: var(--link); }
	.zh-dialog label { display: block; font-size: 12px; color: var(--text-muted); margin-bottom: 3px; }
	.zh-dialog input, .zh-dialog textarea, .zh-dialog select {
		width: 100%; box-sizing: border-box; padding: 6px 8px;
		background: var(--input-bg); border: 1px solid var(--border); border-radius: 4px;
		color: var(--text); font-size: 13px; margin-bottom: 10px;
	}
	.zh-dialog textarea { resize: vertical; min-height: 80px; font-family: monospace; }
	.zh-dialog .zh-row { display: flex; gap: 8px; align-items: center; margin-bottom: 8px; }
	.zh-dialog .zh-row input, .zh-dialog .zh-row select { margin-bottom: 0; }
	.zh-dialog .zh-buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
	.zh-dialog button {
		padding: 6px 16px; border-radius: 4px; border: 1px solid var(--border);
		background: var(--btn-bg); color: var(--text); cursor: pointer; font-size: 13px;
	}
	.zh-dialog button:hover { background: var(--btn-hover); }
	.zh-dialog button.primary { background: var(--btn-primary); border-color: var(--btn-primary-hover); color: var(--btn-primary-text); }
	.zh-dialog button.primary:hover { background: var(--btn-primary-hover); color: var(--btn-primary-text); }
	.zh-dialog button.danger { color: var(--accent); border-color: var(--accent); background: transparent; }
	.zh-dialog button.danger:hover { background: var(--accent-subtle); }
	.zh-dialog .zh-section-label {
		font-size: 12px; font-weight: bold; color: var(--link);
		margin: 12px 0 6px; padding-bottom: 4px; border-bottom: 1px solid var(--border);
	}
	.zh-dialog .zh-port-row { display: flex; gap: 6px; align-items: center; margin-bottom: 6px; }
	.zh-dialog .zh-port-row input { flex: 1; margin-bottom: 0; }
	.zh-dialog .zh-port-row select { flex: 1; margin-bottom: 0; }
	.zh-dialog .zh-tool-card {
		border: 1px solid var(--border); border-radius: 6px; padding: 10px; margin-bottom: 10px; background: var(--tool-card-bg);
	}
	.zh-dialog .zh-tool-card summary {
		cursor: pointer; font-size: 13px; font-weight: bold; color: var(--tool-card-summary); list-style: none;
	}
	.zh-dialog .zh-hint { font-size: 11px; color: var(--text-dim); margin-bottom: 10px; }
`;

export function ensureDialogStyles(): void {
	if (document.getElementById("zh-dialog-styles")) return;
	const style = document.createElement("style");
	style.id = "zh-dialog-styles";
	style.textContent = DIALOG_STYLES;
	document.head.appendChild(style);
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
