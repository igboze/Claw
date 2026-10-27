import Link from "next/link";

export default function NotFound() {
  return (
    <div className="py-20 text-center">
      <h1 className="text-2xl font-semibold">Not found</h1>
      <p className="mt-2 text-muted-foreground">That page does not exist.</p>
      <Link href="/" className="mt-4 inline-block text-primary hover:underline">
        Back home
      </Link>
    </div>
  );
}
