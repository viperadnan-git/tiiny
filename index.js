const fs = require('fs');
const { Deta } = require('deta');
const express = require('express');
const cheerio = require('cheerio');

const deta = Deta();
const db = deta.Base('tiiny');

const port = process.argv[3] || process.env.PORT || 8080;
const hostname = process.argv[2] || process.env.HOST || '127.0.0.1';
const DOMAIN_NAME = process.env.DOMAIN_NAME || `${process.env.DETA_PATH}.deta.dev`;
const app = express();

String.prototype.isalnum = function() {
    var regExp = /^[A-Za-z0-9]+$/;
    return (this.match(regExp));
};

async function getRandomString(length = 8) {
    var randomChars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    var result = '';
    for (var i = 0; i < length; i++) {
        result += randomChars.charAt(Math.floor(Math.random() * randomChars.length));
    }
    return result;
}

async function stringIsAValidUrl(str) {
    var pattern = "(?:(?:https?|ftp):\/\/)?[\w/\-?=%.]+\.[\w/\-?=%.]+";
    return str.match(pattern);
}

app.use('/static', express.static('assets/static'));
app.use(async (req, res, next) => {
    res.header("X-Powered-By", "viperadnan");
    console.log(`${req.method} ${req.path}`);
    next();
});
app.use(express.json());

var cheerio_load = cheerio.load(fs.readFileSync("./assets/index.html"));
cheerio_load('#key-box-label').text(`https://${DOMAIN_NAME}/`);
var mainHTML = cheerio_load.html();

app.get("/", async (req, res) => {
    res.setHeader('Content-type', 'text/html');
    res.end(mainHTML);
});

app.post("/api", async (req, res) => {
    if (req.body.url && await stringIsAValidUrl(req.body.url)) {
        if (!req.body.key) {
            req.body.key = await getRandomString();
        }
        if (req.body.key.isalnum()) {
            try {
                res.end((await db.insert({
                    key: req.body.key, url: req.body.url
                })).key);
            } catch(e) {
                res.status(400).end(e.message);
            }
        } else {
            res.status(400).end('Key can only contain alphanumeric characters');
        }
    } else {
        res.status(400).end('Invalid URL provided');
    }
});

app.get('/:key', async (req, res) => {
    let data = await db.get(req.params.key);
    if (data) {
        res.redirect(data.url);
    } else {
        res.status(404).sendFile('assets/404.html', {
            root: __dirname
        });
    }
});

/** Deta will listen it automatically
app.listen(port, hostname, async () => {
    console.log(`Listening at ${hostname}:${port}`);
});
**/

module.exports = app;