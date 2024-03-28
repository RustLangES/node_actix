// Code from https://expressjs.com/es/starter/hello-world.html
import express from 'express'

const app = express()

app.get('/', (_, res) => {
    res.send('Hello World!')
})

app.listen(3000)

