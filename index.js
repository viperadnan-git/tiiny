const dotenv = require('dotenv');
dotenv.config()

const express = require('express');
const cookieParser = require('cookie-parser')
const { Deta } = require('deta');
const path = require('path');
const JoiSchemas = require('./src/schema')
const { JoiValidate } = require('./src/middlewares')
const { generateDomainName, generateKey } = require('./src/utils')


const deta = Deta(process.env.DETA_PROJECT_KEY);
const db = deta.Base(process.env.DETA_BASE_NAME || 'tiiny');


const port = process.argv[3] || process.env.PORT || 8000;
const hostname = process.argv[2] || process.env.HOST || '127.0.0.1';
const website_domain = generateDomainName(hostname, port);
const website_name = process.env.WEBSITE_NAME || 'Tiiny';
const app = express();



app.use(async (req, res, next) => {
    console.log(`${req.method} ${req.path}`);
    next();
});
app.use(express.json());
app.use(cookieParser());
app.set('view engine', 'ejs');
app.set('views', path.join(__dirname, './src/web'));
app.use('/static', express.static('src/web/assets'));


app.get("/", async (req, res) => {
    let mode = req.cookies.__TIINY_MODE;
    res.setHeader('Content-Type', 'text/html');
    res.render('index', {
        mode: mode,
        website_name: website_name,
        website_domain: website_domain
    })
});


app.post("/api", JoiValidate(JoiSchemas.api), async (req, res) => {
    if (!req.body.key) {
        req.body.key = generateKey()
    }
    try {
        let key = (await db.insert(req.body)).key
        res.json({
            key: key,
            url: `http://${website_domain}/${key}`
        });
    } catch (error) {
        res.status(400).json({ error: error.message });
    }
});


app.get('/:key', async (req, res) => {
    let data = await db.get(req.params.key);
    if (data) {
        res.redirect(data.url);
    } else {
        let mode = req.cookies.__TIINY_MODE;
        res.status(404).render('404', {
            mode: mode
        })
    }
});


if (process.env.DETA_RUNTIME) {
    module.exports = app;
} else {
    app.listen(port, hostname, async () => {
        console.log(`Listening at ${hostname}:${port}`);
    });
}