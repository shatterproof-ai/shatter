import type { Socket as NetSocket, Server as NetServer } from "net";
import type { IncomingMessage, ServerResponse } from "http";
import type { Readable, Writable, Transform, Duplex } from "stream";
import type { ChildProcess } from "child_process";
import type { ReadStream, WriteStream } from "fs";

export function handleSocket(sock: NetSocket): void {
  void sock;
}

export function handleNetServer(srv: NetServer): void {
  void srv;
}

export function handleHttp(req: IncomingMessage, res: ServerResponse): void {
  void req;
  void res;
}

export function handleStreams(r: Readable, w: Writable): void {
  void r;
  void w;
}

export function handleTransformDuplex(t: Transform, d: Duplex): void {
  void t;
  void d;
}

export function handleChildProcess(cp: ChildProcess): void {
  void cp;
}

export function handleFsStreams(rs: ReadStream, ws: WriteStream): void {
  void rs;
  void ws;
}
