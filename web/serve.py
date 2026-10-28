#!/usr/bin/env python3
"""Static server for the demo with the cross-origin isolation headers SharedArrayBuffer needs.

Usage: python3 web/serve.py [port]   (serves the web/dist/ build)
"""
import functools
import http.server
import os
import sys


class Handler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
        self.send_header('Cache-Control', 'no-store')
        super().end_headers()


if __name__ == '__main__':
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
    directory = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'dist')
    if not os.path.isfile(os.path.join(directory, 'index.html')):
        sys.exit('Build the Svelte app first: npm --prefix web run build')
    h = functools.partial(Handler, directory=directory)
    print(f'http://localhost:{port}/')
    http.server.ThreadingHTTPServer(('127.0.0.1', port), h).serve_forever()
