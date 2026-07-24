import { getDb } from "../src/lib/mongodb.js";

async function runIndexPerformanceEvaluation() {
  console.log("=== Initializing Catalog Index Optimization Evaluation ===");
  const db = await getDb();
  const collection = db.collection("materials");

  // Inject sample records to simulate low-latency catalog scaling
  console.log("Seeding runtime sample nodes for optimization trace...");
  const sampleCount = await collection.countDocuments();
  if (sampleCount === 0) {
    const mockData = Array.from({ length: 50 }).map((_, idx) => ({
      title: `Advanced Soroban Smart Contract Development Guide Vol ${idx}`,
      description:
        "Learn how to build scalable DeFi applications on the Stellar Network utilizing Rust tooling engines.",
      category: "Blockchain",
      price: parseFloat((10.5 + idx * 2.5).toFixed(2)),
      createdAt: new Date(),
    }));
    await collection.insertMany(mockData);
  }

  // Evaluate Query Performance Using Explain Plan
  console.log("\nExecuting Execution Plan Query Trace via explain()...");
  const searchQuery = {
    $text: { $search: "Soroban" },
    category: "Blockchain",
    price: { $gte: 10.0 },
  };

  const explainPlan = await collection
    .find(searchQuery)
    .explain("executionStats");
  const executionStages = explainPlan.executionStats.executionStages;

  // Track if any full-table collection scans occurred
  const hasColScan = JSON.stringify(explainPlan).includes("COLLSCAN");
  console.log(
    `- Query Strategy Strategy Type: ${hasColScan ? "COLLSCAN (Sequential Scan)" : "IXSCAN (Indexed Key Hit)"}`,
  );
  console.log(
    `- Total Documents Inspected: ${explainPlan.executionStats.totalDocsExamined}`,
  );

  // Evaluate Latency Benchmarks
  console.log("\nRunning High-Traffic Latency Latency Stress Test...");
  const startTime = performance.now();

  // Loop execution bounds to test connection pool stability under continuous load
  for (let i = 0; i < 20; i++) {
    await collection.find(searchQuery).toArray();
  }

  const endTime = performance.now();
  const averageLatency = (endTime - startTime) / 20;
  console.log(
    `- Average Query Round-Trip Latency: ${averageLatency.toFixed(2)}ms`,
  );

  // Validate System SLA Metrics
  if (averageLatency < 100 && !hasColScan) {
    console.log(
      "\n ACCEPTANCE CRITERIA SATISFIED: Search returns under 100ms utilizing index hits.",
    );
    process.exit(0);
  } else {
    console.error(
      "\n SLA CRITERIA FAILED: Query latency exceeded performance targets or missed index hits.",
    );
    process.exit(1);
  }
}

runIndexPerformanceEvaluation().catch((err) => {
  console.error("Critical Failure executing index evaluation script:", err);
  process.exit(1);
});
