import { describe, expect, it } from "vitest";
import {
  DIAGNOSE_BUSY_CAP_MS,
  shouldAutoScheduleAi,
  shouldTripWatchdog,
  type AutoScheduleInput,
} from "./diagnoseAutoSchedule";

/** The state the AI tab is in right after a fresh Diagnose load, no AI yet. */
const baseline: AutoScheduleInput = {
  mainTabAi: true,
  hasProject: true,
  hasDiagnosis: true,
  sessionOk: false,
  hasAiAnalysis: false,
  hasAiSoftError: false,
  analysisBusy: false,
  watchdogTripped: false,
};

describe("shouldAutoScheduleAi (AI tab auto-run gate)", () => {
  it("schedules on a fresh crashing project with no AI result yet", () => {
    expect(shouldAutoScheduleAi(baseline)).toBe(true);
  });

  it("does not schedule outside the AI tab / without project / before diagnosis", () => {
    expect(shouldAutoScheduleAi({ ...baseline, mainTabAi: false })).toBe(false);
    expect(shouldAutoScheduleAi({ ...baseline, hasProject: false })).toBe(false);
    expect(shouldAutoScheduleAi({ ...baseline, hasDiagnosis: false })).toBe(false);
  });

  it("does not schedule when the session is healthy (nothing to analyze)", () => {
    expect(shouldAutoScheduleAi({ ...baseline, sessionOk: true })).toBe(false);
  });

  it("does not schedule while a run is in flight or results exist", () => {
    expect(shouldAutoScheduleAi({ ...baseline, analysisBusy: true })).toBe(false);
    expect(shouldAutoScheduleAi({ ...baseline, hasAiAnalysis: true })).toBe(false);
    expect(shouldAutoScheduleAi({ ...baseline, hasAiSoftError: true })).toBe(false);
  });

  it("REGRESSION: a tripped watchdog blocks auto-rescheduling (infinite Analyzing loop)", () => {
    // Watchdog cleared the busy flags and nulled the source stamps, but no
    // AI result or soft error ever landed — the old code re-scheduled here,
    // forever. The trip flag must break that cycle.
    expect(shouldAutoScheduleAi({ ...baseline, watchdogTripped: true })).toBe(false);
  });

  it("manual reset (watchdogTripped=false) re-enables scheduling", () => {
    expect(shouldAutoScheduleAi({ ...baseline, watchdogTripped: true })).toBe(false);
    expect(shouldAutoScheduleAi({ ...baseline, watchdogTripped: false })).toBe(true);
  });
});

describe("shouldTripWatchdog", () => {
  it("never trips while idle", () => {
    expect(shouldTripWatchdog({ busySinceMs: 0, nowMs: Date.now() })).toBe(false);
  });

  it("does not trip before the cap", () => {
    const busySince = 1_000_000;
    expect(
      shouldTripWatchdog({ busySinceMs: busySince, nowMs: busySince + DIAGNOSE_BUSY_CAP_MS - 1 }),
    ).toBe(false);
  });

  it("trips exactly at the cap and after", () => {
    const busySince = 1_000_000;
    expect(shouldTripWatchdog({ busySinceMs: busySince, nowMs: busySince + DIAGNOSE_BUSY_CAP_MS })).toBe(true);
    expect(
      shouldTripWatchdog({ busySinceMs: busySince, nowMs: busySince + DIAGNOSE_BUSY_CAP_MS + 5_000 }),
    ).toBe(true);
  });

  it("honours a custom cap", () => {
    expect(shouldTripWatchdog({ busySinceMs: 100, nowMs: 350, capMs: 250 })).toBe(true);
    expect(shouldTripWatchdog({ busySinceMs: 100, nowMs: 349, capMs: 250 })).toBe(false);
  });
});
