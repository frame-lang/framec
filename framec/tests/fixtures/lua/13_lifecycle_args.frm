@@system LifecycleArgs {
    interface:
        load(n: number, label: string)
        total(): number
        tag(): string

    machine:
        $Idle {
            load(n: number, label: string) {
                -> (n, label) $Active
            }
        }

        $Active {
            $>(count: number, name: string) {
                self.sum = count + 1;
                self.label = name;
            }
            total(): number {
                @@:(self.sum)
                return
            }
            tag(): string {
                @@:(self.label)
                return
            }
        }

    domain:
        sum: number = 0
        label: string = ""
}
