export function Switch({
  id,
  checked,
  label,
  disabled = false,
  onChange,
}: {
  id: string;
  checked: boolean;
  label: string;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="settings-page__switch">
      <input id={id} type="checkbox" checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} />
      <span aria-hidden="true" />
      <span className="visually-hidden">{label}</span>
    </label>
  );
}
