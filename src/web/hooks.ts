import { createContext, useContext, useEffect, useState } from "react";
import type { Snapshot } from "../shared/api.ts";
import type { ApiClient, Connection } from "./api.ts";
import { ApiError } from "./api.ts";

export const ClientContext = createContext<ApiClient | null>(null);

export function useClient(): ApiClient {
  const c = useContext(ClientContext);
  if (!c) throw new Error("ClientContext is missing");
  return c;
}

export function useHash(): string {
  const [hash, setHash] = useState(() => location.hash);
  useEffect(() => {
    const on = () => setHash(location.hash);
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  }, []);
  return hash;
}

/** The current time in ms, refreshed every `ms`. */
export function useNow(ms = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(t);
  }, [ms]);
  return now;
}

export function useSnapshot(client: ApiClient): { snapshot: Snapshot | null; connection: Connection } {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [connection, setConnection] = useState<Connection>("reconnecting");
  useEffect(() => client.subscribe(setSnapshot, setConnection), [client]);
  return { snapshot, connection };
}

export interface Loaded<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
}

/** Runs `load` when `key` changes. A newer key drops the result of an older one. */
export function useLoad<T>(load: () => Promise<T>, key: string): Loaded<T> {
  const [state, setState] = useState<Loaded<T>>({ data: null, error: null, loading: true });
  useEffect(() => {
    let live = true;
    setState({ data: null, error: null, loading: true });
    load().then(
      (data) => live && setState({ data, error: null, loading: false }),
      (err: unknown) =>
        live &&
        setState({
          data: null,
          error: err instanceof ApiError ? `${err.problem.code}: ${err.problem.detail}` : String(err),
          loading: false,
        }),
    );
    return () => {
      live = false;
    };
    // `key` stands for everything `load` closes over.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return state;
}
