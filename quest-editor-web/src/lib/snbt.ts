/**
 * SNBT (Stringified NBT) parser and serializer.
 * Supports: unquoted keys, numeric suffixes (d/f/L/b), comments (// and /* *​/),
 * single-quoted strings, trailing commas.
 */

export type SnbtValue =
  | string
  | number
  | boolean
  | SnbtValue[]
  | { [key: string]: SnbtValue };

// ─── Strip comments + trailing commas ───────────────────────────

function stripSnbtComments(input: string): string {
  let out = "";
  let i = 0;
  const bytes = input;
  let inStr = false;
  let strCh = '"';

  while (i < bytes.length) {
    const c = bytes[i];

    if (inStr) {
      out += c;
      if (c === "\\" && i + 1 < bytes.length) {
        out += bytes[i + 1];
        i += 2;
        continue;
      }
      if (c === strCh) {
        inStr = false;
      }
      i++;
      continue;
    }

    // Line comment
    if (c === "/" && i + 1 < bytes.length && bytes[i + 1] === "/") {
      while (i < bytes.length && bytes[i] !== "\n") i++;
      continue;
    }

    // Block comment
    if (c === "/" && i + 1 < bytes.length && bytes[i + 1] === "*") {
      i += 2;
      while (i + 1 < bytes.length && !(bytes[i] === "*" && bytes[i + 1] === "/")) i++;
      i = Math.min(i + 2, bytes.length);
      continue;
    }

    // String start (double or single quote)
    if (c === '"' || c === "'") {
      inStr = true;
      strCh = c;
      out += '"'; // normalize to double quote
      i++;
      continue;
    }

    out += c;
    i++;
  }

  return out;
}

// ─── Parser ─────────────────────────────────────────────────────

class SnbtParser {
  private chars: string[];
  private pos = 0;

  constructor(text: string) {
    this.chars = [...stripSnbtComments(text)];
  }

  private peek(): string | undefined {
    return this.chars[this.pos];
  }

  private skipWs(): void {
    while (this.pos < this.chars.length && /\s/.test(this.chars[this.pos])) {
      this.pos++;
    }
  }

  parseValue(): SnbtValue {
    this.skipWs();
    const c = this.peek();
    if (c === undefined) throw new Error("SNBT parse: unexpected end of input");
    if (c === "{") return this.parseObject();
    if (c === "[") return this.parseArray();
    if (c === '"') return this.parseString();
    if (c === "-" || /\d/.test(c)) return this.parseNumber();
    if (/[a-zA-Z_]/.test(c)) return this.parseIdentValue();
    throw new Error(`SNBT parse: unexpected char '${c}' at ${this.pos}`);
  }

  private parseObject(): Record<string, SnbtValue> {
    this.pos++; // consume '{'
    const map: Record<string, SnbtValue> = {};
    this.skipWs();
    if (this.peek() === "}") {
      this.pos++;
      return map;
    }
    while (true) {
      this.skipWs();
      const key = this.parseKey();
      this.skipWs();
      if (this.peek() !== ":") {
        throw new Error(`SNBT parse: expected ':' after key at ${this.pos}`);
      }
      this.pos++; // consume ':'
      map[key] = this.parseValue();
      this.skipWs();
      const c = this.peek();
      if (c === ",") {
        this.pos++;
        continue;
      }
      if (c === "}") {
        this.pos++;
        break;
      }
      // SNBT allows whitespace-only separators
    }
    return map;
  }

  private parseArray(): SnbtValue[] {
    this.pos++; // consume '['
    const arr: SnbtValue[] = [];
    this.skipWs();
    if (this.peek() === "]") {
      this.pos++;
      return arr;
    }
    while (true) {
      arr.push(this.parseValue());
      this.skipWs();
      const c = this.peek();
      if (c === ",") {
        this.pos++;
        continue;
      }
      if (c === "]") {
        this.pos++;
        break;
      }
    }
    return arr;
  }

  private parseString(): string {
    this.pos++; // consume opening quote
    let s = "";
    while (this.pos < this.chars.length) {
      const c = this.chars[this.pos];
      this.pos++;
      if (c === '"') return s;
      if (c === "\\") {
        if (this.pos < this.chars.length) {
          const e = this.chars[this.pos];
          this.pos++;
          switch (e) {
            case "n": s += "\n"; break;
            case "t": s += "\t"; break;
            case "r": s += "\r"; break;
            case "\\": s += "\\"; break;
            case '"': s += '"'; break;
            case "'": s += "'"; break;
            case "/": s += "/"; break;
            case "b": s += "\b"; break;
            case "f": s += "\f"; break;
            default: s += e; break;
          }
        }
      } else {
        s += c;
      }
    }
    throw new Error("SNBT parse: unterminated string");
  }

  private parseKey(): string {
    const c = this.peek();
    if (c === '"') return this.parseString();
    if (c && /[a-zA-Z_]/.test(c)) {
      const start = this.pos;
      while (
        this.pos < this.chars.length &&
        /[a-zA-Z0-9_.]/.test(this.chars[this.pos])
      ) {
        this.pos++;
      }
      return this.chars.slice(start, this.pos).join("");
    }
    throw new Error(`SNBT parse: expected key at ${this.pos}`);
  }

  private parseNumber(): number {
    const start = this.pos;
    if (this.peek() === "-") this.pos++;
    while (
      this.pos < this.chars.length &&
      /[0-9.eE+\-]/.test(this.chars[this.pos])
    ) {
      this.pos++;
    }
    // SNBT suffixes: d/D/f/F/l/L/b/B/s/S
    if (
      this.pos < this.chars.length &&
      /[dDfFlLbBsS]/.test(this.chars[this.pos])
    ) {
      this.pos++;
    }
    const raw = this.chars.slice(start, this.pos).join("");
    const numeric = raw.replace(/[dDfFlLbBsS]$/, "");
    const asInt = parseInt(numeric, 10);
    if (!isNaN(asInt) && String(asInt) === numeric) return asInt;
    const asFloat = parseFloat(numeric);
    if (!isNaN(asFloat)) return asFloat;
    throw new Error(`SNBT parse: invalid number '${raw}'`);
  }

  private parseIdentValue(): SnbtValue {
    const start = this.pos;
    while (
      this.pos < this.chars.length &&
      /[a-zA-Z0-9_]/.test(this.chars[this.pos])
    ) {
      this.pos++;
    }
    const word = this.chars.slice(start, this.pos).join("");
    if (word === "true") return true;
    if (word === "false") return false;
    if (word === "Infinity") return Infinity;
    if (word === "-Infinity") return -Infinity;
    if (word === "NaN") return NaN;
    throw new Error(`SNBT parse: unknown identifier '${word}'`);
  }
}

export function parseSnbt(text: string): SnbtValue {
  const parser = new SnbtParser(text);
  const v = parser.parseValue();
  return v;
}

// ─── Serializer ─────────────────────────────────────────────────

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
  if (typeof v === "boolean") return v ? "1b" : "0b";
  if (Array.isArray(v)) {
    if (v.length === 0) return "[]";
    const items = v.map((x) => `${pad}  ${snbtValue(x, pad + "  ")}`);
    return `[\n${items.join(",\n")}\n${pad}]`;
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
