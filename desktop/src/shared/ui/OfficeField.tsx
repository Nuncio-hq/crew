import type * as React from "react";

import {
  OFFICE_FIELD_BOX_CLASS,
  OFFICE_FIELD_LABEL_CLASS,
  OFFICE_SURFACE,
} from "@/shared/layout/officeChrome";
import { cn } from "@/shared/lib/cn";

/**
 * Label above a field box. The label is a sibling of the control, not a
 * wrapper sitting on the input (office chrome / #221).
 */
export function OfficeField({
  label,
  htmlFor,
  children,
  className,
}: {
  label: string;
  htmlFor: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col gap-1", className)}>
      <label
        className={OFFICE_FIELD_LABEL_CLASS}
        data-office-field-label=""
        htmlFor={htmlFor}
      >
        {label}
      </label>
      <div
        className={OFFICE_FIELD_BOX_CLASS}
        data-office-surface={OFFICE_SURFACE.fieldBox}
      >
        {children}
      </div>
    </div>
  );
}
