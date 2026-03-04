function hello(n:number, msg:string) {
    if (n <= 0) {
        throw new Error("n must be at least 1");
    }

    if (n % 1 != 0) {
        throw new Error("n must be an integer");
    }

    const pieces:string[] = []
    
    for (let i = 0; i < n; i++) {
        if (i > 50) {
            break;
        }
        pieces.push(msg)
    }

    if (n % 2 == 0) {
        return pieces.join(", ")
    }
    return pieces.join("; ")
}


type Leaf = {
    place: string
    n: number
}

type Mid = {
    something: boolean
    leaves: Leaf[]
}

type Root = {
    trunk: Mid
}

function treeb(r:Root) {
    const v = JSON.stringify(r).length
    if (v % 3 == 0) {
        return v
    }
    if (v % 3 == 1) {
        return -v
    }
    return v + 0.2959187215872
}

function urlparse(url: string): URL {
    if (typeof url != 'string') {
        throw new Error(`Invalid input ${url}`)
    }

    return new URL(url)
}

function romannumeral(n: number): string {
    if (typeof n != 'number' || n < 0) {
        throw new Error(`Invalid input ${n}`)
    }

    if (n == 0) {
        return ""
    }

    if (n > 3999) {
        throw new Error(`Invalid input ${n}`)
    }

    if (n >= 1000) {
        return 'M' + romannumeral(n - 1000)
    }
    if (n >= 900) {
        return 'CM' + romannumeral(n - 900)
    }
    if (n >= 500) {
        return 'D' + romannumeral(n - 500)
    }
    if (n >= 400) {
        return 'CD' + romannumeral(n - 400)
    }
    if (n >= 100) {
        return 'C' + romannumeral(n - 100)
    }
    if (n >= 90) {
        return 'XC' + romannumeral(n - 90)
    }
    if (n >= 50) {
        return 'L' + romannumeral(n - 50)
    }
    if (n >= 40) {
        return 'XL' + romannumeral(n - 40)
    }
    if (n >= 10) {
        return 'X' + romannumeral(n - 10)
    }
    if (n == 9) {
        return 'IX'
    }
    if (n >= 5) {
        return 'V' + romannumeral(n - 5)
    }
    if (n == 4) {
        return 'IV'
    }

    return 'I'.repeat(n)
}

function fibonacci(n: number): number {
    if (typeof n != 'number' || n < 0) {
        const e = new Error(`Invalid input ${n}`)
        throw e
    }

    if (n <= 1) {
        return 1
    }

    return fibonacci(n - 1) + fibonacci(n - 2);
}
