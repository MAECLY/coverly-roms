import { Slot } from '@radix-ui/react-slot';
import {
  createContext,
  forwardRef,
  useContext,
  useId,
  useLayoutEffect,
  useMemo,
  useState,
  type ComponentPropsWithoutRef,
  type ElementRef,
  type HTMLAttributes,
  type ReactNode,
} from 'react';

import { cn } from '@/lib/utils';

import { Label } from './label';

interface FieldContextValue {
  id: string;
  descriptionId: string;
  errorId: string;
  invalid: boolean;
  disabled: boolean;
  hasDescription: boolean;
  hasError: boolean;
  setHasDescription: (on: boolean) => void;
  setHasError: (on: boolean) => void;
}

const FieldContext = createContext<FieldContextValue | null>(null);

/** The ids and state of the enclosing `<Field>`; throws outside one. */
export function useField(): FieldContextValue {
  const ctx = useContext(FieldContext);
  if (!ctx) throw new Error('Field parts must be used inside <Field>');
  return ctx;
}

export interface FieldProps extends HTMLAttributes<HTMLDivElement> {
  /** The control's id. Generated when omitted. */
  id?: string;
  /** Marks the control `aria-invalid`. Also true while a FieldError renders. */
  invalid?: boolean;
  disabled?: boolean;
  /** `horizontal` puts the control before the label (Checkbox, Switch). */
  orientation?: 'vertical' | 'horizontal';
}

/**
 * Label + control + description + error, wired without react-hook-form:
 * FieldControl receives `id`, `aria-describedby` (description, then error)
 * and `aria-invalid`, and FieldLabel points at it.
 */
export const Field = forwardRef<HTMLDivElement, FieldProps>(
  (
    {
      id: idProp,
      invalid = false,
      disabled = false,
      orientation = 'vertical',
      className,
      ...props
    },
    ref,
  ) => {
    const auto = useId();
    const id = idProp ?? `field-${auto.replace(/:/g, '')}`;
    const [hasDescription, setHasDescription] = useState(false);
    const [hasError, setHasError] = useState(false);
    const value = useMemo<FieldContextValue>(
      () => ({
        id,
        descriptionId: `${id}-description`,
        errorId: `${id}-error`,
        invalid: invalid || hasError,
        disabled,
        hasDescription,
        hasError,
        setHasDescription,
        setHasError,
      }),
      [id, invalid, disabled, hasDescription, hasError],
    );
    return (
      <FieldContext.Provider value={value}>
        <div
          ref={ref}
          data-orientation={orientation}
          data-invalid={value.invalid || undefined}
          data-disabled={disabled || undefined}
          className={cn(
            orientation === 'vertical'
              ? 'grid gap-1.5'
              : 'grid grid-cols-[auto_1fr] items-start gap-x-2.5 gap-y-0.5 [&>[data-field-control]]:row-span-2 [&>[data-field-control]]:mt-px',
            className,
          )}
          {...props}
        />
      </FieldContext.Provider>
    );
  },
);
Field.displayName = 'Field';

export const FieldLabel = forwardRef<
  ElementRef<typeof Label>,
  ComponentPropsWithoutRef<typeof Label>
>(({ className, ...props }, ref) => {
  const { id, disabled } = useField();
  return (
    <Label
      ref={ref}
      htmlFor={id}
      className={cn(disabled && 'text-cr-text-disabled', className)}
      {...props}
    />
  );
});
FieldLabel.displayName = 'FieldLabel';

/** Wraps exactly one control and injects the Field's ids into it. */
export const FieldControl = forwardRef<
  HTMLElement,
  { children: ReactNode; className?: string }
>(({ children, ...props }, ref) => {
  const {
    id,
    descriptionId,
    errorId,
    invalid,
    disabled,
    hasDescription,
    hasError,
  } = useField();
  const describedBy =
    [hasDescription && descriptionId, hasError && errorId]
      .filter(Boolean)
      .join(' ') || undefined;
  return (
    <Slot
      ref={ref}
      id={id}
      data-field-control=""
      aria-describedby={describedBy}
      aria-invalid={invalid || undefined}
      {...(disabled ? { disabled: true } : {})}
      {...props}
    >
      {children}
    </Slot>
  );
});
FieldControl.displayName = 'FieldControl';

export const FieldDescription = forwardRef<
  HTMLParagraphElement,
  HTMLAttributes<HTMLParagraphElement>
>(({ className, ...props }, ref) => {
  const { descriptionId, setHasDescription } = useField();
  useLayoutEffect(() => {
    setHasDescription(true);
    return () => setHasDescription(false);
  }, [setHasDescription]);
  return (
    <p
      ref={ref}
      id={descriptionId}
      className={cn('m-0 text-caption text-cr-text-secondary', className)}
      {...props}
    />
  );
});
FieldDescription.displayName = 'FieldDescription';

/** Renders (and links) only when it has content. */
export const FieldError = forwardRef<
  HTMLParagraphElement,
  HTMLAttributes<HTMLParagraphElement>
>(({ className, children, ...props }, ref) => {
  const { errorId, setHasError } = useField();
  const show =
    children !== undefined &&
    children !== null &&
    children !== false &&
    children !== '';
  useLayoutEffect(() => {
    setHasError(show);
    return () => setHasError(false);
  }, [show, setHasError]);
  if (!show) return null;
  return (
    <p
      ref={ref}
      id={errorId}
      role="alert"
      className={cn('m-0 text-caption font-medium text-cr-danger', className)}
      {...props}
    >
      {children}
    </p>
  );
});
FieldError.displayName = 'FieldError';
