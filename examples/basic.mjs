import actix from "../index.js"

const app = actix();

app.get("/", (req) => {
  // console.log("Index!", req);
});
app.get("/hello", async (req) => {
  // console.log("Hello World!", req);

  if (req.body) {
    // console.log("HAS BODY: ", await req.json());
  }
});

process.on("SIGINT", () => {
  console.log("CLOSING");
  process.exit(0);
});

await app.listen(3000, (server) => {
  console.log(`LISTENING on ${server.hostname}:${server.port}`);
});
