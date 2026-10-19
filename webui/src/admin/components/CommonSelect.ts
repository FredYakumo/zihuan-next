export interface CommonSelectOption {
  value: string;
  label: string;
  /** Optional secondary text rendered right-aligned inside the dropdown list. */
  description?: string;
}

export interface CommonSelectProps {
  options: CommonSelectOption[];
  placeholder?: string;
  clearable: boolean;
  filterable: boolean;
  disabled: boolean;
}

export interface CommonSelectEmit {
  (event: "change", value: string | undefined): void;
}

export function useCommonSelect(_props: CommonSelectProps, emit: CommonSelectEmit) {
  function onChange(value: unknown) {
    emit("change", typeof value === "string" ? value : undefined);
  }

  return { onChange };
}
