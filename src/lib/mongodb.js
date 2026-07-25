// Resolves: Configure efficient MongoDB connection pooling in the Next.js API routes to handle concurrent requests.
import { cpus } from "node:os";
import { MongoClient } from "mongodb";
import { REQUIRED_INDEXES } from "./backend/schemaContracts.js";

const uri = process.env.MONGODB_URI;

// Scale pool size to active CPU count so connection limits grow with the host.
// Allow env overrides for environments where auto-detection is insufficient.
const CPU_COUNT = cpus().length;
const maxPoolSize = parseInt(
  process.env.MONGODB_MAX_POOL_SIZE || String(CPU_COUNT * 5),
  10,
);
const minPoolSize = parseInt(
  process.env.MONGODB_MIN_POOL_SIZE || String(CPU_COUNT),
  10,
);
const serverSelectionTimeoutMS = parseInt(
  process.env.MONGODB_TIMEOUT_MS || "5000",
  10,
); // Fail fast
// How often the driver pings each server to confirm connectivity.
const heartbeatFrequencyMS = parseInt(
  process.env.MONGODB_HEARTBEAT_MS || "10000",
  10,
);

const globalForMongo = globalThis;

function getClientPromise() {
  if (!uri) {
    const errorMsg = "MONGODB_URI is not set in environment variables";
    console.error(`[Database Error]: ${errorMsg}`);
    throw new Error(errorMsg);
  }

  // Reuse the client across hot reloads in dev, but only connect on demand.
  if (!globalForMongo._mongoClientPromise) {
    try {
      const client = new MongoClient(uri, {
        maxPoolSize,
        minPoolSize,
        serverSelectionTimeoutMS,
        heartbeatFrequencyMS,
        maxIdleTimeMS: 30000,
      });

      globalForMongo._mongoClientPromise = client.connect().catch((error) => {
        console.error(
          "[Database Connection Error]: Failed to connect to MongoDB cluster:",
          error,
        );
        // Reset global cache so subsequent requests can try connecting again
        globalForMongo._mongoClientPromise = null;
        throw error;
      });
    } catch (error) {
      console.error(
        "[Database Initialization Error]: Failed to initialize MongoClient:",
        error,
      );
      throw error;
    }
  }

  return globalForMongo._mongoClientPromise;
}

let indexesCreated = false;

async function ensureIndexes(db) {
  for (const [collectionName, indexes] of Object.entries(REQUIRED_INDEXES)) {
    const collection = db.collection(collectionName);
    for (const { keys, options } of indexes) {
      try {
        await collection.createIndex(keys, options);
      } catch (error) {
        console.error(
          `[Database Index Error]: Failed to create index on "${collectionName}" (${JSON.stringify(keys)}):`,
          error,
        );
      }
    }
  }
  console.log("MongoDB indexes ensured successfully.");
}

export async function getDb() {
  try {
    const client = await getClientPromise();
    // When DB name is in connection string, driver selects it automatically.
    // Otherwise, fallback to "eduvault".
    const dbName = process.env.MONGODB_DB || "eduvault";
    const db = client.db(dbName);

    if (!indexesCreated) {
      indexesCreated = true;
      ensureIndexes(db).catch((err) =>
        console.error("[Database Index Async Error]:", err),
      );
    }

    return db;
  } catch (error) {
    console.error(
      "[Database Retrieval Error]: Could not acquire database instance:",
      error,
    );
    throw error;
  }
}
