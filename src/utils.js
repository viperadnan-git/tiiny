const key_length = process.env.DEFAULT_KEY_LENGTH || 8;


const generateDomainName = (hostname, port) => {
    if (process.env.DOMAIN_NAME) {
        return process.env.DOMAIN_NAME
    } else if (process.env.DETA_RUNTIME) {
        return process.env.DETA_PATH + '.deta.dev'
    } else {
        return hostname + ":" + port
    }
}

const generateKey = () => {
    var randomChars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    var result = '';
    for (var i = 0; i < key_length; i++) {
        result += randomChars.charAt(Math.floor(Math.random() * randomChars.length));
    }
    return result;
}

module.exports = {
    generateDomainName,
    generateKey
}