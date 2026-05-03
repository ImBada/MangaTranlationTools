import type express from "express";
import type { JobEvent } from "../shared/types";
import { writeLog } from "./logger";

export type ActiveJob = {
  id: string;
  abortController: AbortController;
  cleanup?: () => Promise<void>;
  lastEvent?: JobEvent;
};

let activeJob: ActiveJob | null = null;
const eventClients = new Set<express.Response>();

export function getActiveJob(): ActiveJob | null {
  return activeJob;
}

export function setActiveJob(job: ActiveJob | null): void {
  activeJob = job;
}

export function updateActiveJob(id: string, patch: Partial<Omit<ActiveJob, "id">>): void {
  if (activeJob?.id === id) {
    activeJob = { ...activeJob, ...patch };
  }
}

export function recordJobEvent(id: string, event: JobEvent): void {
  if (activeJob?.id === id) {
    activeJob.lastEvent = event;
  }
}

export function emitJobEvent(event: JobEvent): void {
  writeLog(event.status === "failed" ? "error" : event.status === "cancelled" ? "warn" : "info", `job:${event.kind}:${event.status}`, event);
  for (const client of eventClients) {
    client.write(`data: ${JSON.stringify(event)}\n\n`);
  }
}

export function addEventClient(response: express.Response): void {
  eventClients.add(response);
}

export function removeEventClient(response: express.Response): void {
  eventClients.delete(response);
}

export async function cancelActiveJob(): Promise<boolean> {
  if (!activeJob) {
    return false;
  }

  const job = activeJob;
  emitJobEvent({
    id: job.id,
    kind: job.lastEvent?.kind ?? "gemma-analysis",
    status: "cancelling",
    progressText: "작업 취소 중",
    progressCurrent: job.lastEvent?.progressCurrent,
    progressTotal: job.lastEvent?.progressTotal,
    pageIndex: job.lastEvent?.pageIndex,
    pageTotal: job.lastEvent?.pageTotal,
    attempt: job.lastEvent?.attempt,
    attemptTotal: job.lastEvent?.attemptTotal
  });
  job.abortController.abort();
  await job.cleanup?.();
  return true;
}

export function abortActiveJobForShutdown(): void {
  if (activeJob) {
    activeJob.abortController.abort();
    void activeJob.cleanup?.();
  }
}
