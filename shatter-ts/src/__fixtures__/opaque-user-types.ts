/** User-defined class with the same name as a Node.js type — should NOT become opaque. */
class Socket {
  id: number;
  constructor(id: number) {
    this.id = id;
  }
}

export function handleUserSocket(sock: Socket): number {
  return sock.id;
}
