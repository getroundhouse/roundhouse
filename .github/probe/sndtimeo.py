"""How long blocking sends to a peer that never reads take, under SO_SNDTIMEO of 200 ms, as tls.rs's writer sends."""
import socket, struct, time

def trial(per_send):
    server = socket.socket(); server.bind(("127.0.0.1", 0)); server.listen(1)
    client = socket.create_connection(server.getsockname()); peer, _ = server.accept()
    client.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    data = memoryview(b"x" * (16 << 20)); sent = 0; start = time.monotonic(); deadline = start + 0.2
    longest = 0.0; calls = 0
    while sent < len(data):
        left = deadline - time.monotonic()
        if left <= 0:
            outcome = "deadline passed between sends"; break
        client.setsockopt(socket.SOL_SOCKET, socket.SO_SNDTIMEO, struct.pack("qq", 0, int(left * 1e6)))
        t = time.monotonic()
        try:
            n = client.send(data[sent:sent + per_send])
        except (BlockingIOError, TimeoutError) as e:
            longest = max(longest, time.monotonic() - t); outcome = f"send timed out ({type(e).__name__})"; break
        longest = max(longest, time.monotonic() - t); calls += 1
        sent += n
    else:
        outcome = "all sent"
    print(f"{per_send >> 10} KB a send: {outcome} after {time.monotonic() - start:.2f}s, {sent / 1048576:.1f} MB in {calls} sends, longest send {longest:.2f}s")
    client.close(); peer.close(); server.close()

for per_send in [16 << 10, 64 << 10, 16 << 20, 16 << 10, 64 << 10, 16 << 20]:
    trial(per_send)
