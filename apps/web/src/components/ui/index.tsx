// Minimal shadcn/ui-style primitives. Replace with `npx shadcn add` components freely;
// the class names follow the same tokens.

import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import type { ButtonHTMLAttributes, HTMLAttributes, InputHTMLAttributes, ReactNode } from "react";

export const cn = (...v: ClassValue[]) => twMerge(clsx(v));

export function Card({ className, ...p }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("rounded-lg border border-border bg-card p-5 text-card-foreground", className)} {...p} />;
}

export function CardTitle({ className, ...p }: HTMLAttributes<HTMLHeadingElement>) {
  return <h2 className={cn("mb-3 text-sm font-semibold uppercase tracking-wide text-muted-foreground", className)} {...p} />;
}

type Variant = "default" | "outline" | "ghost";
export function Button({ className, variant = "default", ...p }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      className={cn(
        "inline-flex h-9 items-center justify-center rounded-md px-4 text-sm font-medium transition-colors disabled:pointer-events-none disabled:opacity-50",
        variant === "default" && "bg-primary text-primary-foreground hover:opacity-90",
        variant === "outline" && "border border-border hover:bg-muted",
        variant === "ghost" && "hover:bg-muted",
        className,
      )}
      {...p}
    />
  );
}

export function Input({ className, ...p }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={cn("h-9 w-full rounded-md border border-border bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-primary/40", className)} {...p} />;
}

const tones: Record<string, string> = {
  Active: "bg-success/15 text-success",
  Queued: "bg-warning/15 text-warning",
  Executing: "bg-warning/15 text-warning",
  Executed: "bg-primary/15 text-primary",
  Defeated: "bg-danger/15 text-danger",
  Failed: "bg-danger/15 text-danger",
  Cancelled: "bg-muted text-muted-foreground",
  Expired: "bg-muted text-muted-foreground",
  Exhausted: "bg-muted text-muted-foreground",
};
export function Badge({ children, tone }: { children: ReactNode; tone?: string }) {
  return (
    <span className={cn("inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium", tones[tone ?? String(children)] ?? "bg-muted text-muted-foreground")}>
      {children}
    </span>
  );
}

export function Progress({ value }: { value: number }) {
  return (
    <div className="h-2 w-full overflow-hidden rounded-full bg-muted" role="progressbar" aria-valuenow={Math.round(value)}>
      <div className="h-full rounded-full bg-primary" style={{ width: `${value}%` }} />
    </div>
  );
}

export function Stat({ label, value, hint }: { label: string; value: ReactNode; hint?: ReactNode }) {
  return (
    <Card>
      <div className="text-xs uppercase tracking-wide text-muted-foreground">{label}</div>
      <div className="mt-1 text-2xl font-semibold tabular-nums">{value}</div>
      {hint && <div className="mt-1 text-xs text-muted-foreground">{hint}</div>}
    </Card>
  );
}

export function State({ loading, error, empty }: { loading?: boolean; error?: string; empty?: string }) {
  if (loading) return <p className="text-sm text-muted-foreground">Loading…</p>;
  if (error) return <p className="text-sm text-danger">Could not load: {error}</p>;
  if (empty) return <p className="text-sm text-muted-foreground">{empty}</p>;
  return null;
}
