// Code from https://fastify.dev/docs/latest/Guides/Getting-Started/#your-first-server
import Fastify from 'fastify'
const fastify = Fastify()

// Declare a route
fastify.get('/', function(_, reply) {
    reply.send({ hello: 'world' })
})

// Run the server!
fastify.listen({ port: 3000 }, function(err, _) {
    if (err) {
        fastify.log.error(err)
        process.exit(1)
    }
    // Server is now listening on ${address}
})
