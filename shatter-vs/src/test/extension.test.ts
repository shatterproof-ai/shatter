import { mkdtempSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

import { faker } from '@faker-js/faker';
import * as ts from 'typescript';
import { shatterAutotest } from "../core/shatter";
import { stringFakerses, optionVariantsMedium } from '../core/seed';
namespace Nomen {
    export type Z = {
        a: number,
    };
}


describe('scratch space', () => {
    it('does', () => {
        const sourceCode = `
        type M = {
            mm: Map<number, any>,
        };

        type nono = number;

        namespace Nomen {
            export type Z = {
                a: number,
            };
        }
        
        interface MMM {
            nn: Map<number, any>,
        }

        function hello(m:Map<string, number>, m2:M, n:MMM, nz:Nomen.Z, mnmnmn:Map<string, Map<number, Map<boolean, any>>>, nooonoooo:nono) {
        }

        function zort(x:"a"|"b"|"c") {
        }

        const y = () => 0;
        `;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const sourceFilePath = join(tempdir, 'zert.ts');
        writeFileSync(sourceFilePath, sourceCode);

        const program = ts.createProgram([sourceFilePath], {});

        const checker = program.getTypeChecker();

        const source = program.getSourceFile(sourceFilePath);


        const dumpType = (type: ts.Type, line: number) => {
            return `TYPE (${line}): type.aliasSymbol?.getName() = ${type.aliasSymbol?.getName()
                }; type.pattern?.getText() = ${type.pattern?.getText()
                }; type.flags = ${type.flags
                }; type.symbol.getName() = ${type.symbol?.getName()
                }; type to string = ${checker.typeToString(type)
                }; alias type args = ${type.aliasTypeArguments?.map((t) => checker.typeToString(t)).join(', ')})
                }`;
        };

        const visitor = (node: ts.Node) => {
            if (ts.isFunctionLike(node)) {
                node.parameters.forEach((param) => {
                    const typeNode = param.type;
                    if (!typeNode) {
                        return;
                    }

                    const dumpTypeNode = (typeNode: ts.TypeNode, soFar:string[]=[]) => {
                        const baseType = checker.getTypeFromTypeNode(typeNode);
                        const line = ts.getLineAndCharacterOfPosition(source!, typeNode.pos).line;
            
                        console.log(`PARAMETER NODE Name: ${param.name.getText()} ${soFar.join('.')}; flags = ${typeNode.flags
                            }; text = ${typeNode.getText()
                            }; dumpTypes = ${dumpType(baseType, line)
                            }`);
            
                        if (ts.isTypeReferenceNode(typeNode)) {
                            const referencedType = checker.getTypeFromTypeNode(typeNode);
                            if (referencedType) {}
                            console.log(`TYPE REFERENCE NODE: ${typeNode.typeName.getText()}; typeNode.flags = ${typeNode.flags /*}; FQN = ${checker.getFullyQualifiedName(referencedType.symbol)
                            */}; dumpTypes = ${dumpType(referencedType, -1)
                                }`);
                            typeNode.typeArguments?.forEach((typeArg, i) => {
                                dumpTypeNode(typeArg, soFar.concat([`typeArg${i}`]));
                            });
                        }
                        if (ts.isUnionTypeNode(typeNode)) {
                            typeNode.types.forEach((type, i) => {
                                dumpTypeNode(type, soFar.concat([`type${i}`]));
                            });
                        }
                        if (ts.isStringLiteralLike(typeNode)) {
                            console.log(`STRING LITERAL NODE: ${typeNode.text}`);
                        }
            
                    };
            
                    dumpTypeNode(typeNode);
                });
                return;
            }
            if (ts.isInterfaceDeclaration(node)) {

            }
            if (ts.isTypeAliasDeclaration(node)) {
            }
            node.getChildren().forEach((child) => {
                ts.visitNode(child, visitor);
            });
            return node;
        };
        ts.visitNode(source, visitor);

    });
});


describe('extension', () => {
    it('should pass', async () => {
        const sourceCode = `
        function hello(n:number, msgKey:string, messages:Map<string, string>, s_unused:Set<number>) {
            if (n <= 0) {
                throw new Error("n must be at least 1");
            }
        
            if (n % 1 != 0) {
                throw new Error("n must be an integer");
            }
        
            const pieces:string[] = []
            
            switch (n) {
                case 3: {
                    console.log("n is 3");
                    break;
                }
                case 6: {
                    console.log("n is 6");
                    break;
                }
                case 10: {
                    console.log("n is 10");
                    break;
                }
                case 15: {
                    console.log("n is 15");
                    break;
                }
                case 40:
                case 41: {
                    console.log("n is 40 or 41");
                    break;
                }
            }

            const msg = messages.get(msgKey) ?? "default message";
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
        `;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));
        const testfile = join(tempdir, 'hello.ts');
        writeFileSync(testfile, sourceCode);

        const functionName = "hello";

        const modulePaths = process.env.NODE_ENV?.split(':') ?? [];

        const shatterproofModuleOverride = "/home/ketan/project/shatter/shatter-vs/src";
        const maxIterations = 500;
        // const maxTime = 120_000;
        const maxTime = 10_000;
        const { executed, instrumented } = await shatterAutotest(modulePaths, testfile, tempdir, functionName, (clusters) => {
            // console.log(`Received clusters ${JSON.stringify(clusters, null, 2)}`);
        }, { shatterproofModuleOverride, maxIterations, maxTime });
        const unexecuted = instrumented.filter((i) => !executed.includes(i));
        console.log(`Executed     ${executed}`);
        console.log(`Instrumented ${instrumented}`);
        console.log(`Missed       ${unexecuted}`);
    });
});

describe('complicated', () => {
    it('should pass', async () => {
        //  TODO: duh
        const testfile = "/home/ketan/project/shatter/examples/typescript/src/query-creator.ts";
        // const testfile = "/home/ketan/project/shatter/examples/typescript/src/query-creator-short.ts";

        const functionName = "constructSearchQuery";

        const modulePaths = ["/home/ketan/project/shatter/examples/typescript/node_modules"];

        const shatterproofModuleOverride = "/home/ketan/project/shatter/shatter-vs/src";
        const maxIterations = 500;
        // const maxTime = 120_000;
        const maxTime = 10_000;

        const tempdir = mkdtempSync(join(tmpdir(), "shatter-test-"));

        const { executed, instrumented } = await shatterAutotest(modulePaths, testfile, tempdir, functionName, (clusters) => {
            // console.log(`Received clusters ${JSON.stringify(clusters, null, 2)}`);
        }, { shatterproofModuleOverride, maxIterations, maxTime });
        const unexecuted = instrumented.filter((i) => !executed.includes(i));
        console.log(`Executed     ${executed}`);
        console.log(`Instrumented ${instrumented}`);
        console.log(`Missed       ${unexecuted}`);
    });
});


const permute = function* (first: [string, (string | number)[]], rest: [string, (string | number)[]][], opts: any = {}): Generator<any, void, unknown> {
    const [optionKey, optionValues] = first;
    for (const value of optionValues) {
        const newOpts = {
            ...opts,
            [optionKey]: value,
        };
        if (rest.length === 0) {
            yield newOpts;
        } else {
            yield* permute(rest[0], rest.slice(1), newOpts);
        }
    }
};

describe('throwaway', () => {
    it('does', () => {
        const strangs: string[] = [];
        Object.entries(stringFakerses).forEach(([domain, generators]) => {
            generators.forEach((generator) => {
                for (let i = 0; i < 1; i++) {
                    const generatorOptionVariants: any = optionVariantsMedium[generator];

                    const fd = faker[domain as keyof typeof faker];
                    const f = fd[generator as keyof typeof fd] as any;

                    if (generatorOptionVariants) {
                        const all: [string, any[]][] = Object.entries(generatorOptionVariants);
                        const [first, ...rest] = all;
                        for (const opts of permute(first, rest)) {
                            const v = f(opts);
                            strangs.push(v);
                        }
                    } else {
                        const v = f(generatorOptionVariants);
                        strangs.push(v);
                    }
                }
            });
        });
        console.log(JSON.stringify(strangs));
    });
});