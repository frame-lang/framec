@@system ReturnExplicit {
    interface:
        decide(score: number): string

    machine:
        $Judging {
            decide(score: number): string {
                if score >= 60 then
                    @@:return("pass")
                end
                @@:return("fail")
            }
        }
}
