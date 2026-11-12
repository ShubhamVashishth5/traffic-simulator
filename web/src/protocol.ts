/**
 * Viewer protocol helpers (the message types are generated from the Rust types into
 * `protocol.gen.ts`; see `crates/marg-server/src/protocol.rs`).
 */
import { type Table, tableFromIPC } from "apache-arrow";

/** Protocol version this viewer speaks; the server refuses others. */
export const PROTOCOL_VERSION = 2;

/** One decoded agent frame: columns as typed arrays (positions relative to `Hello.origin`). */
export interface AgentFrame {
  t: number;
  step: number;
  /** Agents in the network (the frame may hold fewer: culled to the viewport). */
  total: number;
  colour: string;
  n: number;
  id: BigUint64Array;
  x: Float32Array;
  y: Float32Array;
  heading: Float32Array;
  length: Float32Array;
  width: Float32Array;
  cls: Uint8Array;
  value: Float32Array;
}

function col<T>(table: Table, name: string): T {
  const c = table.getChild(name);
  if (!c) throw new Error(`frame has no column ${name}`);
  return c.toArray() as T;
}

/** Decode a binary frame (an Arrow IPC stream with one record batch). */
export function decodeFrame(bytes: ArrayBuffer | Uint8Array): AgentFrame {
  const table = tableFromIPC(bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes));
  const meta = table.schema.metadata;
  const version = Number(meta.get("protocol"));
  if (version !== PROTOCOL_VERSION) throw new Error(`frame protocol ${version}, viewer speaks ${PROTOCOL_VERSION}`);
  return {
    t: Number(meta.get("t")),
    step: Number(meta.get("step")),
    total: Number(meta.get("total")),
    colour: meta.get("colour") ?? "class",
    n: table.numRows,
    id: col<BigUint64Array>(table, "id"),
    x: col<Float32Array>(table, "x"),
    y: col<Float32Array>(table, "y"),
    heading: col<Float32Array>(table, "heading"),
    length: col<Float32Array>(table, "length"),
    width: col<Float32Array>(table, "width"),
    cls: col<Uint8Array>(table, "class"),
    value: col<Float32Array>(table, "colour"),
  };
}
