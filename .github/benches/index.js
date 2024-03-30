const fs = require('fs')
const { exec } = require('child_process')
const path = require('path')

const CURRENT_BENCH = process.env.CURRENT_BENCH
const CACHE_FILE = path.join(__dirname, 'benchmark_cache.json')

const threads = 4
const connections = 500
const duration = 30

const convertUnits = (n) => {
    const units = ["B", "KB", "MB", "GB"]
    let exp = 0

    while (n >= 1024 && exp < units.length - 1) {
        n /= 1024
        exp++
    }

    return `${n.toFixed(2)} ${units[exp]}`
}

function runBenchmark(callback) {
    const command = `rewrk --json -t ${threads} -c ${connections} -d ${duration}s -h http://localhost:3000`
    exec(command, (error, stdout, stderr) => {
        if (error) {
            console.error(`Error al ejecutar el benchmark: ${error.message}`)
            return
        }
        if (stderr) {
            console.error(`Error de stderr: ${stderr}`)
            return
        }
        const raw = stdout.trim()
        const obj = JSON.parse(raw)

        callback({
            transfer: {
                total: obj.transfer_total,
                rate: obj.transfer_rate,
            },
            requests: {
                total: obj.requests_total,
                avg: obj.requests_avg,
            },
            latencies: {
                min: obj.latency_min,
                max: obj.latency_max,
                stdev: obj.latency_std_deviation,
                avg: obj.latency_avg,
            }
        })
    })
}

function saveToCache(data, save) {
    if (!save) return

    fs.readFile(CACHE_FILE, 'utf8', (err, fileData) => {
        if (err) {
            console.error(`Error al leer el archivo de caché: ${err.message}`)
            return
        }

        let cache = {}
        try {
            cache = JSON.parse(fileData)
        } catch (e) {
            console.error(`Error al parsear el archivo de caché: ${e.message}`)
            return
        }

        cache[CURRENT_BENCH] = data
        fs.writeFile(CACHE_FILE, JSON.stringify(cache, null, 2), (err) => {
            if (err) {
                console.error(`Error al escribir en el archivo de caché: ${err.message}`)
                return
            }
            console.log('Resultados guardados en el caché.')
        })
    })
}

function showDiff(name, obj1, obj2) {
    // Obtener las claves de los objetos
    const showResults = name == CURRENT_BENCH
    const keys = new Set([...Object.keys(obj1 || {}), ...Object.keys(obj2 || {})])

    // Crear la tabla
    let table = showResults
        ? `**${name}**\n| Parametro | Old | New | Diff |\n|---|---|---|---|\n`
        : `**${name}**\n| Parametro | Valor |\n|---|---|\n`

    // Recorrer las claves
    for (const key of keys) {
        // Obtener los valores de la clave en ambos objetos
        const subkeys = new Set([...Object.keys(obj1[key]), ...Object.keys(obj2[key])])

        for (const subkey of subkeys) {
            const v1 = obj1[key][subkey] || " "
            const v2 = obj2[key][subkey]
            const b1 = key == 'transfer' ? convertUnits(v1) : v1
            const b2 = key == 'transfer' ? convertUnits(v2) : v2
            // Resaltar las diferencias
            if (v1 !== v2) {
                const diff = Math.max(v1, v2) - Math.min(v1, v2)
                const diffConv = key == 'transfer' ? convertUnits(diff) : diff
                const sign = diff < v2 ? '+' : '-'
                table += showResults
                    ? `| ${key} ${subkey} | ${b1} | **${b2}** | ${sign} ${diffConv} |\n`
                    : `| ${key} ${subkey} | ${b2} |\n`
            } else {
                table += showResults
                    ? `| ${key} ${subkey} | ${b1} | ${b2} |\n`
                    : `| ${key} ${subkey} | ${b2} |\n`
            }
        }
    }

    // Imprimir la tabla
    console.log(table)
}

function main(save) {
    runBenchmark((res) => {
        fs.readFile(CACHE_FILE, 'utf8', (err, fileData) => {
            if (err) {
                console.error(`Error al leer el archivo de caché: ${err.message}`)
                return
            }

            let cache = {}
            try {
                cache = JSON.parse(fileData)
            } catch (e) {
                console.error(`Error al parsear el archivo de caché: ${e.message}`)
                return
            }

            for (const f of Object.keys(cache)) {
                showDiff(f, cache[f], res)
            }

            saveToCache(res, save)
        })
    })
}

// Ejemplo de uso: node index.js true
// El primer argumento es un booleano que indica si se deben guardar los resultados en el caché.
main(process.argv[2] === 'true')
