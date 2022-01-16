const Joi = require('joi');


const JoiSchemas = {
    api: Joi.object().keys({
        url: Joi.string().uri().required(),
        key: Joi.string().allow(null, false, "").empty([null, false, ""]).alphanum().max(16),
    })
}

module.exports = JoiSchemas