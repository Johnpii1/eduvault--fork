"use client";

export default function GlobalError({ error, reset }) {
  return (
    <html lang="en">
      <body className="antialiased">
        <main className="min-h-screen flex items-center justify-center bg-[#fffaf6] dark:bg-gray-950 px-4">
          <div className="max-w-lg text-center py-20">
            <h1 className="text-2xl font-bold text-gray-900 dark:text-gray-50 mb-2">
              Application error
            </h1>
            <p className="text-gray-500 dark:text-gray-400 leading-relaxed mb-8">
              {error?.message || "A critical error occurred and the application could not load. Please try again."}
            </p>
            <button
              type="button"
              onClick={reset}
              className="px-6 py-2.5 bg-blue-600 hover:bg-blue-700 text-white font-semibold rounded-xl transition"
            >
              Try again
            </button>
          </div>
        </main>
      </body>
    </html>
  );
}
