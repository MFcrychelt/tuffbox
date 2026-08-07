/**
 * Minimal SNBT serializer for quest chapter raw view / export.
 */

export type SnbtArrayType = "I" | "B" | "L" | "F" | "D";

export interface SnbtTypedArray {
  __snbtArray: SnbtArrayType;
  values: SnbtValue[];
}

export type SnbtValue =
  | string
  | number
  | boolean
  | SnbtValue[]
  | SnbtTypedArray
  | { [key: string]: SnbtValue };

export function isSnbtTypedArray(v: unknown): v is SnbtTypedArray {
  return (
    typeof v === "object" &&
    v !== null &&
    !Array.isArray(v) &&
    typeof (v as SnbtTypedArray).__snbtArray === "string" &&
    Array.isArray((v as SnbtTypedArray).values)
  );
}

function snbtString(s: string): string {
  return `"${s.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/\n/g, "\\n")}"`;
}

function snbtValue(v: SnbtValue, pad: string): string {
  if (v === null || v === undefined) return '""';
  if (typeof v === "string") return snbtString(v);
  if (typeof v === "number") {
    if (Number.isInteger(v)) return `${v}L`;
    return `${v}d`;
  }
  if (typeof v === "boolean") return v ? "true" : "false";
  if (Array.isArray(v)) {
    if (v.length === 0) return "[]";
    const items = v.map((x) => `${pad}  ${snbtValue(x, pad + "  ")}`);
    return `[\n${items.join(",\n")}\n${pad}]`;
  }
  if (isSnbtTypedArray(v)) {
    const type = v.__snbtArray;
    if (v.values.length === 0) return `[${type};]`;
    const items = v.values.map((x) => `${pad}  ${snbtValue(x, pad + "  ")}`);
    return `[${type};\n${items.join(",\n")}\n${pad}]`;
  }
  // object
  const entries = Object.entries(v);
  if (entries.length === 0) return "{}";
  const lines = entries.map(
    ([k, val]) => `${pad}  ${k}: ${snbtValue(val, pad + "  ")}`
  );
  return `{\n${lines.join(",\n")}\n${pad}}`;
}

export function serializeSnbt(v: SnbtValue): string {
  return snbtValue(v, "");
}
