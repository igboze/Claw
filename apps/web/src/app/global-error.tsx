"use client";

export default function GlobalError({ reset }: { error: Error; reset: () => void }) {
  return (
    <html lang="en">
      <body style={{ fontFamily: "system-ui", padding: "4rem", textAlign: "center" }}>
        <h1>Something went wrong</h1>
        <button onClick={reset}>Try again</button>
      </body>
    </html>
  );
}
