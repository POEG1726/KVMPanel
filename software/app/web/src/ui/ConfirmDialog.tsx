import { useEffect, useId, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from './Button';
import s from './ui.module.css';

interface Props {
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
  onConfirm(): void;
  onCancel(): void;
}

/** Modal yes/no. Focus starts on Cancel so Enter never confirms a destructive action by accident. */
export function ConfirmDialog({ title, message, confirmLabel, danger, onConfirm, onCancel }: Props) {
  const { t } = useTranslation();
  const id = useId();
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => cancel.current?.focus(), []);
  return (
    <div className={s.mask} onKeyDown={(e) => e.key === 'Escape' && onCancel()}>
      <div role="dialog" aria-modal="true" aria-labelledby={id} className={s.dialog}>
        <h2 id={id}>{title}</h2>
        <p>{message}</p>
        <div className={s.actions}>
          <Button ref={cancel} onClick={onCancel}>
            {t('common.cancel')}
          </Button>
          <Button variant={danger ? 'danger' : 'primary'} onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
