// Mirrors the DTOs in the `contracts` Rust crate. Kept as plain types (not
// generated) since the surface is small; if it grows, revisit codegen.

export interface CaptureListItem {
  event_id: string;
  transcript_text: string;
  occurred_at: string;
}

export interface EntitySummary {
  id: string;
  entity_type: string;
  name: string;
  current_summary: string | null;
}

export interface SearchResult {
  capture_event_id: string;
  transcript_text: string;
  occurred_at: string;
  score: number;
  related_entities: EntitySummary[];
}

const BASE_URL = import.meta.env.VITE_API_BASE_URL ?? "http://localhost:8080";

async function getJson<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE_URL}${path}`);
  if (!res.ok) {
    throw new Error(`${path} failed: ${res.status} ${res.statusText}`);
  }
  return (await res.json()) as T;
}

export function listCaptures(): Promise<CaptureListItem[]> {
  return getJson<CaptureListItem[]>("/captures");
}

export function search(query: string, limit = 20): Promise<SearchResult[]> {
  const params = new URLSearchParams({ query, limit: String(limit) });
  return getJson<SearchResult[]>(`/search?${params}`);
}
