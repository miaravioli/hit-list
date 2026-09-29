-- CREATE TABLES --

CREATE TABLE category (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "category" TEXT NOT NULL UNIQUE
);

CREATE TABLE task (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "task" TEXT NOT NULL,
    "date" DATETIME,
    "status" TEXT CHECK("status" IN ('add', 'commit', 'push')) DEFAULT 'add',
    "source" TEXT CHECK("source" IN ('table', 'receipt')) DEFAULT 'table'
);

CREATE TABLE connect (
    "category_id" INTEGER,
    "task_id" INTEGER,
    FOREIGN KEY("category_id") REFERENCES "category"("id") ON DELETE CASCADE,
    FOREIGN KEY("task_id") REFERENCES "task"("id") ON DELETE CASCADE,
    PRIMARY KEY("category_id", "task_id")
);

CREATE TABLE daily_plan (
    "id" INTEGER PRIMARY KEY AUTOINCREMENT,
    "text" TEXT NOT NULL UNIQUE,
    "priority" INTEGER NOT NULL UNIQUE,
    "status" INTEGER NOT NULL
);

-- CREATE VIEWS --

CREATE VIEW listAll AS
    SELECT task."id", category."category", task."task", task."date", task."status"
    FROM "task"
        LEFT JOIN "connect" ON "task"."id" = "connect"."task_id"
        LEFT JOIN "category" ON "category"."id" = "connect"."category_id"
    WHERE task."source" = 'table'
    ORDER BY "date";

CREATE VIEW standard AS
    SELECT "id", "category", "task", "date", "status"
    FROM "listAll"
    WHERE "status" != 'push'
    ORDER BY "date";

CREATE VIEW timely AS
    SELECT "id", "category", "task", "date", "status"
    FROM "listAll"
    WHERE "date" >= CURRENT_TIMESTAMP
        AND "date" < datetime(CURRENT_TIMESTAMP, '+7 days')
    ORDER BY "date";

CREATE VIEW byCategory AS
    SELECT "id", "category", "task", "date", "status"
    FROM "listAll"
    ORDER BY "category", "date";