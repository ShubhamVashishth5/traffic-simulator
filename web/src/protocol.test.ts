import { Field, RecordBatch, Schema, Struct, Table, makeData, makeVector, tableToIPC } from "apache-arrow";
import { describe, expect, it } from "vitest";
import { decodeFrame } from "./protocol";

function frame(protocol: string) {
  const n = 3;
  const f32 = (vals: number[]) => makeVector(Float32Array.from(vals));
  const cols = {
    id: makeVector(BigUint64Array.from([1n, 2n, (1n << 60n) + 5n])),
    x: f32([0, 1, 2]),
    y: f32([0, 0, 1]),
    heading: f32([0, 0, 0]),
    length: f32([4, 1.9, 10]),
    width: f32([1.7, 0.6, 2.5]),
    class: makeVector(Uint8Array.from([5, 2, 7])),
    colour: f32([10, 12, 8]),
  };
  const fields = Object.entries(cols).map(([k, v]) => new Field(k, v.type, false));
  const meta = new Map([
    ["protocol", protocol],
    ["t", "12.5"],
    ["step", "125"],
    ["total", "7"],
    ["colour", "speed"],
  ]);
  const schema = new Schema(fields, meta);
  const data = makeData({ type: new Struct(fields), length: n, nullCount: 0, children: Object.values(cols).map((v) => v.data[0]) });
  return tableToIPC(new Table(schema, [new RecordBatch(schema, data)]), "stream");
}

describe("decodeFrame", () => {
  it("reads columns and metadata, keeping 64-bit ids exact", () => {
    const f = decodeFrame(frame("1"));
    expect(f.n).toBe(3);
    expect(f.total).toBe(7);
    expect(f.t).toBe(12.5);
    expect(f.colour).toBe("speed");
    expect(f.id[2]).toBe((1n << 60n) + 5n);
    expect(Array.from(f.cls)).toEqual([5, 2, 7]);
    expect(f.length[2]).toBe(10);
  });
  it("refuses another protocol version", () => {
    expect(() => decodeFrame(frame("2"))).toThrow(/protocol/);
  });
});
