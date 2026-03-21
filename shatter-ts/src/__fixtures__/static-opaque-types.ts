import * as net from "net";

// AbstractType: abstract class — cannot be instantiated directly.
export abstract class AbstractService {
  abstract doWork(): void;
}
export function handleAbstract(svc: AbstractService): void {
  svc.doWork();
}

// AbstractType: private constructor — no external code can call new.
export class SingletonClient {
  private constructor(private url: string) { void url; }
  static create(): SingletonClient { return new SingletonClient("url"); }
}
export function handleSingleton(client: SingletonClient): void { void client; }

// NoImplementors: service interface (method-only) with no concrete implementors.
// This is NOT a data-shape interface — it requires a class to implement it.
export interface DataSource {
  query(sql: string): unknown[];
}
export function handleSource(ds: DataSource): void {
  ds.query("select 1");
}

// TransitivelyOpaque: constructor requires net.Socket (already opaque).
export class SocketWrapper {
  constructor(public socket: net.Socket) {}
}
export function handleWrapper(sw: SocketWrapper): void { void sw; }
