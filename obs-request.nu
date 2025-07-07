export def main [req_type: string, req_data: any] {
    influencer request $req_type ($req_data | to json) | parse "{k} {v}" | update v { from json } | transpose --header-row | first
}
