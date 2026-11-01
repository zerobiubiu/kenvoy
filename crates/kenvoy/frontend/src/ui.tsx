import { useEffect, useId, useRef } from 'react';
import type { ReactNode } from 'react';

export function Notice({ kind = 'info', title, children, onClose, role }: {
  kind?: 'info' | 'success' | 'warning' | 'error'; title?: string; children: ReactNode;
  onClose?: () => void; role?: 'alert' | 'status';
}) {
  return <div className={`kv-alert ${kind}`} role={role ?? (kind === 'error' || kind === 'warning' ? 'alert' : 'status')}>
    <div className="kv-alert-content">{title && <p className="kv-alert-title">{title}</p>}{children}</div>
    {onClose && <button type="button" className="kv-button compact" aria-label="关闭提示" onClick={onClose}>关闭</button>}
  </div>;
}

export function Loading({ label = '正在加载…', height = 240 }: { label?: string; height?: number }) {
  return <div className="kv-loading" role="status" aria-label={label} style={{ minHeight: height }}>
    <span>{label}</span><div className="kv-skeleton" aria-hidden="true" />
  </div>;
}

/** 使用浏览器原生模态层约束焦点；关闭后将焦点交回触发控件。 */
export function NativeDialog({ open, onClose, title, children, footer, className = '', closeDisabled = false }: {
  open: boolean; onClose: () => void; title: string; children: ReactNode; footer?: ReactNode;
  className?: string; closeDisabled?: boolean;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const trigger = useRef<HTMLElement | null>(null);
  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (open) {
      trigger.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      if (!element.open) element.showModal();
      element.querySelector<HTMLElement>('[data-autofocus]')?.focus();
    } else if (element.open) {
      element.close();
      if (trigger.current?.isConnected) trigger.current.focus();
    }
    return () => {
      if (element.open) {
        element.close();
        if (trigger.current?.isConnected) trigger.current.focus();
      }
    };
  }, [open]);
  return <dialog ref={dialog} className={`kv-dialog ${className}`} aria-labelledby={titleId}
    onCancel={(event) => { event.preventDefault(); if (!closeDisabled) onClose(); }}
    onKeyDown={(event) => {
      if (!(event.target instanceof HTMLElement)) return;
      const menu = event.target.closest('[role="menu"]');
      if (!menu || !['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
      const items = Array.from(menu.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')).filter(item => !item.disabled);
      if (!items.length) return;
      event.preventDefault();
      const current = items.indexOf(event.target as HTMLButtonElement);
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1
        : (current + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
      items[next]?.focus();
    }}
    onClick={(event) => {
      if (event.target !== event.currentTarget || closeDisabled) return;
      const bounds = event.currentTarget.getBoundingClientRect();
      if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom) onClose();
    }}>
    <h2 id={titleId} className="kv-dialog-title">{title}</h2>
    <div className="kv-dialog-content">{children}</div>
    <div className="kv-dialog-actions">{footer ?? <button className="kv-button outlined" type="button" disabled={closeDisabled} onClick={onClose}>关闭</button>}</div>
  </dialog>;
}
