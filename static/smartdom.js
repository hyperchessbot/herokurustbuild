class SmartdomElement_ {
    flattenProps(props){        
        // a terminal is something that is not an array
        if(!Array.isArray(props)) return props

        let accum = []

        for(let prop of props){
            if(Array.isArray(prop)){
                // flatten array
                for(let item of prop){
                    accum = accum.concat(this.flattenProps(item))                    
                }
            }else{
                accum.push(prop)
            }
        }

        return accum
    }

    constructor(tag, ...props){
        // create dom element from tag, default = div
        this.e = document.createElement(tag || "div");

        // props
        this.props = props || []

        // flatten props
        this.props = this.flattenProps(this.props)

        // string props
        this.stringProps = []

        // other props
        this.otherProps = []

        // sort out props
        for(let prop of this.props){
            if(typeof prop == "string"){
                this.stringProps.push(prop)
            }else{
                this.otherProps.push(prop)
            }
        }

        // add all string props as classes
        for(let sprop of this.stringProps){
            this.ac(sprop)
        }
    }

    // add classes
    ac(...classes){
        // add classes to element classList
        for(let klass of this.flattenProps(classes)){
            this.e.classList.add(klass)
        }
    }

    // remove classes
    rc(...classes){
        // remove classes from element classList
        for(let klass of this.flattenProps(classes)){
            this.e.classList.remove(klass)
        }
    }

    // add style
    as(key, value){
        this.e.style[key] = value
        return this
    }

    // add style in px
    aspx(key, value){
        return this.as(key, value + "px")
    }

    // set innerHTML
    html(content){
        this.e.innerHTML = content
        return this
    }

    // append childs
    a(...childs){
        for(let child of this.flattenProps(childs)){
            this.e.appendChild(child.e)
        }
        return this
    }
}

// div element
class div_ extends SmartdomElement_{
    constructor(...props){
        super("div", props)
    }
}
function div(...props){return new div_(props)}

// LogItem element
class LogItem_ extends SmartdomElement_{
    constructor(...props){
        super("div", props)

        // determine record properties, supply reasonable default
        this.record = this.otherProps[0] || {}

        // set record defaults
        this.record.time = this.record.time || new Date().getTime()
        this.record.msg = this.record.msg || "log message"

        this.timeDiv = div("time").html(this.record.timeFormatted || new Date(this.record.time).toLocaleString())
        this.msgDiv = div("msg").html(this.record.msg)

        this.ac("logitem")

        this.container = div().as("display", "flex").as("align-items", "center").a(this.timeDiv, this.msgDiv)

        this.as("display", "inline-block").a(this.container)
    }
}
function LogItem(...props){return new LogItem_(props)}

let app = LogItem()

//document.getElementById("root").appendChild(app.e)